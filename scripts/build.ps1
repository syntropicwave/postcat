<#
.SYNOPSIS
  Background release builder for postcat.

.DESCRIPTION
  Runs `npm run tauri build` detached so you don't wait on it. Manage it with:

    ./scripts/build.ps1 start            # full build (exe + installers)
    ./scripts/build.ps1 start -NoBundle  # exe only (skips MSI/NSIS, ~30s faster)
    ./scripts/build.ps1 status           # running? elapsed? finished? artifacts
    ./scripts/build.ps1 log -Tail 40     # tail the build log
    ./scripts/build.ps1 stop             # kill the running build (whole tree)
    ./scripts/build.ps1 restart          # stop + start

  State lives in .build/ (git-ignored): build.pid and build.log.
#>
param(
  [Parameter(Position = 0)]
  [ValidateSet("start", "stop", "status", "restart", "log")]
  [string]$Command = "status",

  [switch]$NoBundle,
  [int]$Tail = 30
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$stateDir = Join-Path $repo ".build"
$pidFile = Join-Path $stateDir "build.pid"
$logFile = Join-Path $stateDir "build.log"
$runFile = Join-Path $stateDir "run.ps1"
$exePath = Join-Path $repo "src-tauri\target\release\postcat.exe"
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null

function Get-RunningPid {
  if (-not (Test-Path $pidFile)) { return $null }
  $p = (Get-Content $pidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
  if (-not $p) { return $null }
  $proc = Get-Process -Id ([int]$p) -ErrorAction SilentlyContinue
  if ($proc) { return [int]$p } else { return $null }
}

function Show-Tail([int]$n) {
  if (Test-Path $logFile) { Get-Content $logFile -Tail $n }
  else { "(no log yet)" }
}

function Do-Status {
  $running = Get-RunningPid
  if ($running) {
    $started = $null
    if (Test-Path $logFile) {
      $line = Select-String -Path $logFile -Pattern "__BUILD_START__ (.+)$" | Select-Object -First 1
      if ($line) { $started = [datetime]::Parse($line.Matches[0].Groups[1].Value) }
    }
    $elapsed = if ($started) { "{0:n0}s" -f ((Get-Date) - $started).TotalSeconds } else { "?" }
    Write-Host "STATUS: running (pid $running, elapsed $elapsed)" -ForegroundColor Yellow
    Write-Host "--- last $Tail log lines ---"
    Show-Tail $Tail
    return
  }
  # Not running — did the last build finish?
  $done = if (Test-Path $logFile) { Select-String -Path $logFile -Pattern "__BUILD_DONE__ (\d+) (.+)$" | Select-Object -Last 1 } else { $null }
  if ($done) {
    $code = [int]$done.Matches[0].Groups[1].Value
    $when = $done.Matches[0].Groups[2].Value
    if ($code -eq 0) {
      Write-Host "STATUS: finished OK ($when)" -ForegroundColor Green
      if (Test-Path $exePath) {
        $mb = "{0:n1}" -f ((Get-Item $exePath).Length / 1MB)
        Write-Host "  exe: $exePath ($mb MB)"
      }
      Get-ChildItem (Join-Path $repo "src-tauri\target\release\bundle") -Recurse -Include *.exe, *.msi -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host ("  {0} ({1:n1} MB)" -f $_.FullName, ($_.Length / 1MB)) }
    }
    else {
      Write-Host "STATUS: FAILED (exit $code, $when)" -ForegroundColor Red
      Write-Host "--- last $Tail log lines ---"
      Show-Tail $Tail
    }
  }
  elseif (Test-Path $logFile) {
    Write-Host "STATUS: stopped (interrupted before finishing)" -ForegroundColor Gray
  }
  else {
    Write-Host "STATUS: idle (no build has run)" -ForegroundColor Gray
  }
}

function Do-Stop {
  $running = Get-RunningPid
  if (-not $running) { Write-Host "Not running." -ForegroundColor Gray; return }
  # /T kills the whole tree (cargo, rustc, link.exe, cargo-tauri).
  & taskkill /PID $running /T /F | Out-Null
  Remove-Item $pidFile -ErrorAction SilentlyContinue
  Write-Host "Stopped build (pid $running)." -ForegroundColor Yellow
}

function Do-Start {
  $running = Get-RunningPid
  if ($running) { Write-Host "Already running (pid $running). Use 'restart' to rebuild." -ForegroundColor Yellow; return }

  $buildArgs = if ($NoBundle) { "-- --no-bundle" } else { "" }

  # Inner runner: set PATH (GNU patch + cargo), run the build, bracket the log
  # with markers so `status` can report start time and exit code.
  $runner = @"
`$env:Path = "C:\Program Files\Git\usr\bin;`$env:USERPROFILE\.cargo\bin;`$env:Path"
Set-Location "$repo"
"__BUILD_START__ `$(Get-Date -Format o)" | Out-File -FilePath "$logFile" -Encoding utf8
try {
  npm run tauri build $buildArgs *>> "$logFile"
  `$code = `$LASTEXITCODE
} catch {
  `$_ | Out-File -Append "$logFile"; `$code = 1
}
"__BUILD_DONE__ `$code `$(Get-Date -Format o)" | Out-File -Append "$logFile"
"@
  Set-Content -Path $runFile -Value $runner -Encoding utf8

  $proc = Start-Process pwsh -ArgumentList "-NoProfile", "-File", $runFile `
    -WindowStyle Hidden -PassThru
  $proc.Id | Set-Content $pidFile
  $mode = if ($NoBundle) { "exe only" } else { "exe + installers" }
  Write-Host "Started build ($mode) in background (pid $($proc.Id))." -ForegroundColor Green
  Write-Host "Check with: ./scripts/build.ps1 status"
}

switch ($Command) {
  "start" { Do-Start }
  "stop" { Do-Stop }
  "restart" { Do-Stop; Start-Sleep -Milliseconds 500; Do-Start }
  "status" { Do-Status }
  "log" { Show-Tail $Tail }
}
