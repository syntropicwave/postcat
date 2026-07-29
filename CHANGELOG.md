# Changelog

User-facing release notes, short and in English. `build.ps1 release` publishes
the `## <version>` section matching `tauri.conf.json` to the GitHub release and
the updater manifest, and refuses to release without one. Convention: every
shipped task bumps the patch version (package.json, tauri.conf.json,
Cargo.toml, Cargo.lock) and adds a bullet to the top section.

## 0.1.13 — 2026-07-29

- Responses survive an app restart: each tab reloads its last response (or
  error) from history on startup
- The response meta line shows when the request was last executed (time
  today, date + time otherwise; hover for the full timestamp)

## 0.1.12 — 2026-07-29

- The in-editor search panel (Ctrl+F) is readable now: larger input and
  buttons, app-styled colors instead of the tiny library defaults

## 0.1.11 — 2026-07-28

- The request editor opens on the Body section by default, and each tab
  remembers its own active section across switches and restarts (it used to
  reset to Params)

## 0.1.10 — 2026-07-27

- Paste an escaped JSON body (`{\"a\":1}` or a quoted string literal, even
  double-encoded) and it decodes and pretty-prints automatically; the
  Beautify button understands the same forms

## 0.1.9 — 2026-07-27

- Show the full response body: large JSON responses (over 2 MB) now
  pretty-print instead of rendering as one endless highlighted line
- Warn with a banner when a response was cut at the capture cap, with a
  pointer to the Settings knob that raises it

## 0.1.8 — 2026-07-17

- Accent color picker in Settings: bronze, sapphire, indigo, teal, burgundy,
  amethyst

## 0.1.7 — 2026-07-16

- New default look: warm bronze accent

## 0.1.6 — 2026-07-16

- Reopening a history entry restores its response (or error), not just the
  request draft

## 0.1.5 — 2026-07-16

- Relaunch automatically after an update-driven Windows restart

## 0.1.4 — 2026-07-15

- Failed requests show a stage pipeline (DNS → TCP → TLS → send → receive)
  with the failing stage highlighted and a hint
- Aligned the address-row action buttons

## 0.1.3 — 2026-07-15

- Check for updates periodically, not only at launch

## 0.1.2 — 2026-07-15

- Detailed request errors: classified causes with hints instead of a bare
  reqwest message

## 0.1.1 — 2026-07-13

- First public release: requests, collections, searchable history, and the
  auto-updater itself
- Don't stretch a lone tab to full width
