<#
.SYNOPSIS
  Background release builder + auto-deploy for postcat.

.DESCRIPTION
  You RUN the app from   app\postcat.exe   (the deploy target).
  The build always compiles to target\release (a different file), so it never
  clashes with the running app. After a successful build the fresh exe is
  copied into app\postcat.exe — immediately if the app is closed, or the moment
  you close it if it's still open ("deploy on close").

    ./scripts/build.ps1 start            # build (exe + installers) → deploy on close
    ./scripts/build.ps1 start -NoBundle  # exe only (skips MSI/NSIS)
    ./scripts/build.ps1 status           # build phase + deploy state + versions
    ./scripts/build.ps1 log  -Tail 40    # tail the build log
    ./scripts/build.ps1 deploy           # force the swap now (waits if app is open)
    ./scripts/build.ps1 run              # launch app\postcat.exe
    ./scripts/build.ps1 stop | restart

  State lives in .build\ (git-ignored). The deployed app lives in app\.
#>
param(
  [Parameter(Position = 0)]
  [ValidateSet("start", "stop", "status", "restart", "log", "deploy", "run")]
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
$appDir = Join-Path $repo "app"
$deployExe = Join-Path $appDir "postcat.exe"
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null

function Is-Locked([string]$p) {
  if (-not (Test-Path $p)) { return $false }
  try { $f = [System.IO.File]::Open($p, 'Open', 'ReadWrite', 'None'); $f.Close(); return $false }
  catch { return $true }
}

# Copy src → dst, then verify by hash. The OS can hold the handle for a beat
# after the app window closes, so a plain Copy-Item may silently no-op. Give a
# short grace pause, then retry until the destination actually matches.
function Copy-Verified([string]$src, [string]$dst) {
  Start-Sleep -Milliseconds 500
  for ($i = 0; $i -lt 6; $i++) {
    try { Copy-Item $src $dst -Force -ErrorAction Stop } catch { Start-Sleep -Seconds 1; continue }
    if ((Get-FileHash $src).Hash -eq (Get-FileHash $dst).Hash) { return $true }
    Start-Sleep -Seconds 1
  }
  return $false
}

function Get-RunningPid {
  if (-not (Test-Path $pidFile)) { return $null }
  $p = Get-Content $pidFile -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $p) { return $null }
  if (Get-Process -Id ([int]$p) -ErrorAction SilentlyContinue) { return [int]$p } else { return $null }
}

# Parse the current build cycle from the log: building | deploying | deployed | failed | interrupted | idle
function Get-Phase {
  $running = [bool](Get-RunningPid)
  if (-not (Test-Path $logFile)) { return "idle" }
  $txt = Get-Content $logFile -Raw
  $hasDone = $txt -match "__BUILD_DONE__ (\d+)"
  $code = if ($hasDone) { [int]$Matches[1] } else { -1 }
  $hasDeploy = $txt -match "__DEPLOY_DONE__"
  $deployFail = $txt -match "__DEPLOY_FAIL__"
  if ($running) { if (-not $hasDone) { return "building" } else { return "deploying" } }
  if ($hasDeploy) { return "deployed" }
  if ($deployFail) { return "deployfailed" }
  if ($hasDone) { if ($code -eq 0) { return "built" } else { return "failed" } }
  return "interrupted"
}

function Show-Tail([int]$n) { if (Test-Path $logFile) { Get-Content $logFile -Tail $n } else { "(no log yet)" } }

function Show-Artifacts {
  if (Test-Path $deployExe) {
    $i = Get-Item $deployExe
    Write-Host ("  running copy: {0} ({1:n1} MB, {2})" -f $deployExe, ($i.Length / 1MB), $i.LastWriteTime)
  }
  if (Test-Path $exePath) {
    $i = Get-Item $exePath
    Write-Host ("  fresh build:  {0} ({1:n1} MB, {2})" -f $exePath, ($i.Length / 1MB), $i.LastWriteTime)
  }
  Get-ChildItem (Join-Path $repo "src-tauri\target\release\bundle") -Recurse -Include *.exe, *.msi -ErrorAction SilentlyContinue |
    ForEach-Object { Write-Host ("  installer:    {0} ({1:n1} MB)" -f $_.FullName, ($_.Length / 1MB)) }
}

function Do-Status {
  switch (Get-Phase) {
    "building" { Write-Host "STATUS: building…" -ForegroundColor Yellow; Write-Host "--- last $Tail log lines ---"; Show-Tail $Tail }
    "deploying" {
      $waiting = (Get-Content $logFile -Raw) -match "__DEPLOY_WAIT__"
      if ($waiting) { Write-Host "STATUS: built OK — waiting for app\postcat.exe to close, then it swaps in the new build." -ForegroundColor Cyan }
      else { Write-Host "STATUS: built OK — deploying…" -ForegroundColor Cyan }
      Show-Artifacts
    }
    "deployed" { Write-Host "STATUS: deployed ✓" -ForegroundColor Green; Show-Artifacts }
    "deployfailed" { Write-Host "STATUS: built OK but deploy FAILED — run './scripts/build.ps1 deploy' with the app closed." -ForegroundColor Red; Show-Artifacts }
    "built" { Write-Host "STATUS: built OK (not deployed)" -ForegroundColor Green; Show-Artifacts }
    "failed" { Write-Host "STATUS: FAILED" -ForegroundColor Red; Write-Host "--- last $Tail log lines ---"; Show-Tail $Tail }
    "interrupted" { Write-Host "STATUS: stopped (interrupted)" -ForegroundColor Gray }
    default { Write-Host "STATUS: idle (no build has run)" -ForegroundColor Gray }
  }
}

function Do-Stop([switch]$Quiet) {
  $running = Get-RunningPid
  if (-not $running) { if (-not $Quiet) { Write-Host "Not running." -ForegroundColor Gray }; return }
  & taskkill /PID $running /T /F 2>&1 | Out-Null
  Remove-Item $pidFile -ErrorAction SilentlyContinue
  if (-not $Quiet) { Write-Host "Stopped (pid $running)." -ForegroundColor Yellow }
}

$runnerBody = @'
param([string]$Repo,[string]$Log,[string]$AppDir,[string]$DeployExe,[string]$ExePath,[switch]$NoBundle)
$env:Path = "C:\Program Files\Git\usr\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
Set-Location $Repo
function Locked($p){ if(-not(Test-Path $p)){return $false}; try{$f=[System.IO.File]::Open($p,'Open','ReadWrite','None');$f.Close();$false}catch{$true} }
("__BUILD_START__ " + (Get-Date -Format o)) | Out-File -FilePath $Log -Encoding utf8
if ($NoBundle) { npm run tauri build -- --no-bundle *>> $Log } else { npm run tauri build *>> $Log }
$code = $LASTEXITCODE
("__BUILD_DONE__ " + $code + " " + (Get-Date -Format o)) | Out-File -Append $Log
if ($code -eq 0) {
  ("__DEPLOY_START__ " + (Get-Date -Format o)) | Out-File -Append $Log
  New-Item -ItemType Directory -Force -Path $AppDir | Out-Null
  $warned = $false
  while (Locked $DeployExe) {
    if (-not $warned) { ("__DEPLOY_WAIT__ " + (Get-Date -Format o)) | Out-File -Append $Log; $warned = $true }
    Start-Sleep -Seconds 2
  }
  # The handle lingers briefly after the window closes — pause, then copy and
  # verify by hash, retrying so we never log DONE on a silent no-op.
  Start-Sleep -Milliseconds 500
  $ok = $false
  for ($i = 0; $i -lt 6; $i++) {
    try { Copy-Item $ExePath $DeployExe -Force -ErrorAction Stop } catch { Start-Sleep -Seconds 1; continue }
    if ((Get-FileHash $ExePath).Hash -eq (Get-FileHash $DeployExe).Hash) { $ok = $true; break }
    Start-Sleep -Seconds 1
  }
  if ($ok) { ("__DEPLOY_DONE__ " + $DeployExe + " " + (Get-Date -Format o)) | Out-File -Append $Log }
  else { ("__DEPLOY_FAIL__ " + $DeployExe + " " + (Get-Date -Format o)) | Out-File -Append $Log }
}
'@

function Do-Start {
  if ((Get-Phase) -eq "building") { Write-Host "A build is already compiling. Use 'restart'." -ForegroundColor Yellow; return }
  Do-Stop -Quiet   # clear any finished/waiting runner

  if (Is-Locked $exePath) {
    Write-Host "WARNING: target\release\postcat.exe is locked (something is running it directly)." -ForegroundColor Red
    Write-Host "Run the app from  app\postcat.exe  instead so builds don't clash, then retry." -ForegroundColor Red
    return
  }

  Set-Content -Path $runFile -Value $runnerBody -Encoding utf8
  $a = @("-NoProfile", "-File", $runFile, "-Repo", $repo, "-Log", $logFile,
    "-AppDir", $appDir, "-DeployExe", $deployExe, "-ExePath", $exePath)
  if ($NoBundle) { $a += "-NoBundle" }
  $proc = Start-Process pwsh -ArgumentList $a -WindowStyle Hidden -PassThru
  $proc.Id | Set-Content $pidFile
  $mode = if ($NoBundle) { "exe only" } else { "exe + installers" }
  Write-Host "Building ($mode) in background (pid $($proc.Id)); will deploy to app\postcat.exe on close." -ForegroundColor Green
  Write-Host "Check with: ./scripts/build.ps1 status"
}

# Force the copy now (waits if the app is open).
function Do-Deploy {
  if (-not (Test-Path $exePath)) { Write-Host "No build to deploy — run 'start' first." -ForegroundColor Yellow; return }
  New-Item -ItemType Directory -Force -Path $appDir | Out-Null
  if (Is-Locked $deployExe) { Write-Host "app\postcat.exe is running — close it; it will be replaced." -ForegroundColor Cyan }
  while (Is-Locked $deployExe) { Start-Sleep -Seconds 1 }
  if (Copy-Verified $exePath $deployExe) { Write-Host "Deployed → $deployExe" -ForegroundColor Green }
  else { Write-Host "Deploy FAILED — $deployExe still differs after retries (locked?)." -ForegroundColor Red }
}

function Do-Run {
  if (-not (Test-Path $deployExe)) { Write-Host "app\postcat.exe not built yet — run 'start' first." -ForegroundColor Yellow; return }
  Start-Process $deployExe
  Write-Host "Launched $deployExe"
}

switch ($Command) {
  "start" { Do-Start }
  "stop" { Do-Stop }
  "restart" { Do-Stop; Start-Sleep -Milliseconds 500; Do-Start }
  "status" { Do-Status }
  "log" { Show-Tail $Tail }
  "deploy" { Do-Deploy }
  "run" { Do-Run }
}
