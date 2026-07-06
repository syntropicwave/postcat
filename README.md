# postcat

Local-first desktop API client. History that remembers everything: full-text search over every request and response you ever sent — plus collections, environments, and Postman import.

**Status:** phase 0 (skeleton). See [PLAN.md](PLAN.md) for positioning, stack rationale, and the development roadmap.

## Stack

- [Tauri 2](https://tauri.app) shell, Rust core (`src-tauri/`)
- HTTP engine: reqwest (native process — no CORS, full TLS/proxy control)
- Storage: SQLite (rusqlite, bundled) with FTS5 for history search
- Frontend: React + TypeScript + Vite (`src/`)

## Development

Prerequisites: Node.js 22+, Rust stable (with MSVC build tools on Windows).

```sh
npm install
npm run tauri dev      # run the app with hot reload
```

Checks:

```sh
npm run lint           # eslint + prettier
npm run format         # autofix formatting
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test             # includes FTS5 availability probe
```

`cargo` commands require `dist/` to exist (`npm run build`) because `tauri::generate_context!` validates the frontend bundle path at compile time.

## Layout

- `src/ipc/commands.ts` — the only place that calls Tauri `invoke()`; every backend command gets a typed wrapper here
- `src-tauri/src/store/` — SQLite store + versioned migrations (`src-tauri/migrations/*.sql`, append-only)
- `docs/research/` — Postman feature inventory and competitor stack research behind the plan
