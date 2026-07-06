# Desktop API Client Stack Research: Postman & Alternatives

> Research date: 2026-07-06. Focus: informing the stack choice for postcat — a desktop API client differentiated by high-quality, fully-searchable request history.

---

## 1. Postman

**Stack:** Electron + React + MobX. Confirmed by Postman's own engineering blog: the desktop app team works with "Electron APIs, MobX stores, React," and they built a data-access layer using **Waterline ORM over IndexedDB** with MobX observable stores backing views ([Postman engineering spotlight](https://blog.postman.com/spotlight-on-engineering-kamalakannan/)).

**Local storage:** Chromium's IndexedDB, which is physically a **LevelDB** database on disk — on Windows at `%APPDATA%\Postman\IndexedDB\file__0.indexeddb.leveldb`. So "Postman uses LevelDB" is true but indirect: it's IndexedDB-on-LevelDB, accessed through an ORM (Waterline), through browser security/abstraction layers — a known IndexedDB performance tax ([RxDB analysis](https://rxdb.info/electron-database.html)).

**Why it's considered heavy/slow:**

- A basic instance with one empty collection can consume 200–500 MB RAM; large JS bundles at startup; update-related cache conflicts causing freezes; ongoing platform-specific Electron perf issues (e.g., [macOS 26 issue #13836](https://github.com/postmanlabs/postman-app-support/issues/13836)).
- Feature sprawl (Flows, AI, monitoring, governance) increased bug surface and sync-conflict data loss reports.

**Forced-cloud lesson #1:** Postman sunset Scratch Pad (offline mode) starting **May 15, 2023**, fully removed **September 15, 2023**, requiring sign-in for the normal workspace experience; the replacement "lightweight API client" is a deliberately limited local-only mode ([Postman blog](https://blog.postman.com/announcing-new-lightweight-postman-api-client/)). Postman's stated reason: maintaining two architectures (local + cloud) cost 2–4x dev time. History in signed-in Postman is cloud-synced and search over it is widely considered weak — exactly the gap postcat targets.

## 2. Insomnia (Kong)

**Stack:** Electron + React + TypeScript (monorepo `Kong/insomnia`).

**Storage:** **NeDB** — a pure-JavaScript MongoDB-like embedded document store. All entities (workspaces, requests, responses, environments) are NeDB documents persisted under the app-data dir, loaded into an in-memory NeDB at runtime ([Kong docs](https://developer.konghq.com/insomnia/storage/)). Critically: **NeDB has been unmaintained since ~2016** — a cautionary tale about betting on a niche JS embedded DB. Git Sync serializes documents to a `.insomnia/` directory of YAML/JSON files.

**Sync model:** three tiers — Local Vault (NeDB only), Git Sync, Insomnia Cloud (E2EE sync).

**Forced-cloud lesson #2:** Insomnia 8.0 (Sept 2023) made account login + cloud sync the default path; local projects were silently migrated. A single GitHub discussion drew 340+ reactions ([discussion #6590](https://github.com/Kong/insomnia/discussions/6590), [HN thread](https://news.ycombinator.com/item?id=37680126)). Fallout: the **Insomnium** fork (local-only), mass migration to Bruno, and Kong walking it back with Scratch Pad and local/Git storage options. Takeaway: **local-first is not a feature, it's table stakes in this market** — two incumbents burned users the same way within the same month.

## 3. Bruno

**Stack:** Electron + React (repo `usebruno/bruno`; monorepo also ships `@usebruno/cli` Node CLI). Notably **not** trying to escape Electron — its differentiator is storage philosophy, not runtime.

**Storage:** collections live as **plain-text `.bru` files** (its Bru markup: `meta`, `get/post`, `headers`, `body:json`, `script`, `assert` blocks) in a folder you choose — versioned with Git, diffable in PRs, no proprietary DB for collections at all ([usebruno.com](https://www.usebruno.com/)).

**History — Bruno's weak spot and postcat's opportunity.** Bruno's history is an in-session timeline only; the filesystem model gives it nowhere natural to put response history. Open, heavily-upvoted issues:

- [#411 — persist request/response history between launches](https://github.com/usebruno/bruno/issues/411) (open since 2023; notes sensitive data must live _outside_ the collection dir)
- [#6742 — searchable request history with full request details](https://github.com/usebruno/bruno/issues/6742) ("in Postman you can search history by URL… not possible in Bruno")
- [#4215 — history per request](https://github.com/usebruno/bruno/issues/4215), [#4777 — persist timeline](https://github.com/usebruno/bruno/issues/4777), [#7698 — filter history by request](https://github.com/usebruno/bruno/issues/7698)

Direct market validation of postcat's differentiator: the most popular local-first client has years-old open issues asking for exactly this feature.

**Scripting:** originally vm2 (deprecated after critical CVEs — [issue #263](https://github.com/usebruno/bruno/issues/263)); now dual-mode: **Safe Mode = QuickJS compiled to WebAssembly** (no fs/network/require), **Developer Mode = raw Node VM** (`--sandbox=developer`, trust required) ([Bruno docs](https://docs.usebruno.com/get-started/javascript-sandbox), [Sonar security analysis](https://www.sonarsource.com/blog/scripting-outside-the-box-api-client-security-risks-part-2/)).

## 4. Hoppscotch

**Stack:** twelve-package TypeScript monorepo. Frontend: **Vue 3** (originally Nuxt-based, now a Vite/Vue app). Self-host backend: **NestJS 11 + GraphQL (Apollo) + Prisma + PostgreSQL + Redis**. Guest/local state (including history) lives in browser storage via a persistence service; team features require the Postgres backend.

**Browser limitations & the interceptor zoo:** as a web app it can't freely make cross-origin requests, so Hoppscotch maintains multiple "interceptors": browser (CORS-limited), proxy, browser extension, and the **Hoppscotch Agent** — a Tauri 2.x tray app with a Rust backend exposing an encrypted local HTTP API (X25519 key exchange) that executes requests natively, bypassing CORS and enabling mTLS/system proxies ([docs](https://docs.hoppscotch.io/documentation/features/interceptor)). This complexity is the cost of web-first — a native desktop app avoids the whole category.

**Desktop app:** migrated **Electron → Tauri**; the team reported bundle size dropping **165 MB → ~8 MB and ~70% memory reduction** ([Hoppscotch desktop announcement](https://hoppscotch.com/blog/introducing-hoppscotch-desktop-application)). Architecture uses a "kernel" abstraction so the same Vue tree runs in browser PWA and Tauri desktop, with a Rust HTTP relay ("hoppscotch-relay") for request execution.

## 5. Yaak

The closest architectural precedent for postcat. Built by **Gregory Schier, Insomnia's original creator**, as his local-first do-over.

**Stack:** **Tauri 2 + Rust + React + TypeScript**. Repo (`mountain-loop/yaak`) is ~56.6% TypeScript, **41.5% Rust** ([GitHub](https://github.com/mountain-loop/yaak)).

**Storage:** all data in **SQLite** via a `yaak-models` Rust crate. Smart hybrid for responses: **response metadata in SQLite, response bodies written to filesystem** at `$APPDATA/responses/` so multi-GB bodies don't bloat the DB. Sync: optional workspace **mirroring to plain files on disk** for Git/Dropbox — SQLite as source of truth, text files as the sync/versioning surface. This SQLite-primary + file-mirror model is arguably the best of Bruno (git-friendliness) and a real DB (queryable history), and extends naturally to FTS.

**Plugins/scripting:** plugins run in an **isolated Node.js sidecar runtime** (`yaaknode`, Node 24.x), communicating with the Rust core over WebSocket with async request/reply events — i.e., he did _not_ embed a JS engine in Rust; he ships Node as a Tauri sidecar.

**Why Tauri (Schier's own words,** [BuildWith.app interview](https://buildwith.app/apps/yaak)**):** liked type safety and security focus; Rust unlocked lower-level networking libraries "especially for gRPC"; after a year+, "it simply feels like a better-designed Electron — things like auto-updates, sidecar binaries, and plugins are so nice to work with," while acknowledging Tauri's youth means bugs and missing features.

**Gap:** Yaak proves the Tauri+SQLite architecture works for this product category but has no FTS story — that's postcat's wedge.

## 6. Apidog, Kreya, RapidAPI (Paw) — brief

- **Apidog:** closed-source; its own marketing claims a "native rendering engine," not Electron — treat as unverified vendor claim. All-in-one design/mock/test platform with cloud sync; not local-first.
- **Kreya:** **C#/.NET backend + Angular frontend + Monaco editor**, rendered in native WebViews via a **fork of SpiderEye** (no local webserver; direct C#↔WebView IPC). Chromium (WebView2) on Windows, WebKit on macOS/Linux — they independently arrived at a "Tauri-shaped" architecture in .NET. Storage: custom git-diffable text format designed for merge-conflict friendliness ([How we built Kreya](https://kreya.app/blog/how-we-built-kreya/)). Strong gRPC support via protobuf reflection.
- **Paw → RapidAPI for Mac:** core engine originally **native Swift/Objective-C, macOS-only**; after the 2021 RapidAPI acquisition a cross-platform version was prototyped with a **JavaScript port of the core engine**. Development effectively stagnated post-acquisition — a lesson that a beloved native client can die from ownership churn, and that single-platform-native limits reach.

## 7. Tauri vs Electron for this app (2025–2026)

| Dimension             | Electron                                          | Tauri 2                                                               |
| --------------------- | ------------------------------------------------- | --------------------------------------------------------------------- |
| Runtime               | Bundles Chromium (~85 MB) + Node (~25 MB) per app | OS webview: **WebView2** (Win), WKWebView (macOS), WebKitGTK (Linux)  |
| Installer             | ~80–150 MB                                        | often < 10 MB (Hoppscotch: 165→8 MB)                                  |
| Idle memory           | ~150–300 MB typical                               | ~30–50 MB typical; Hoppscotch reported −70%                           |
| Backend               | Node.js                                           | Rust                                                                  |
| Rendering consistency | Identical everywhere (you ship the browser)       | Divergent: WebKitGTK lags Chromium; CSS/font quirks; more platform QA |
| Ecosystem             | Deep, mature, huge npm surface                    | Younger; slower Rust compile times; fewer batteries                   |

Sources: [gethopp.app comparison](https://www.gethopp.app/blog/tauri-vs-electron), [DoltHub's Electron vs Tauri](https://www.dolthub.com/blog/2025-11-13-electron-vs-tauri/).

**Specific to an API client, a Rust backend is a structural advantage, not just a perf one:** requests execute in the Rust process via **reqwest/hyper** — no CORS, no browser networking restrictions, full control over TLS/mTLS/proxies/HTTP2/redirects/timing capture. This is why Hoppscotch built a Rust relay+agent and Yaak cites Rust networking (gRPC via tonic) as a key win. **WebView2 on Windows is mature** (Chromium-based, ships with Win10/11, auto-updated); the real webview risk is **Linux WebKitGTK** (rendering quirks, occasional GPU issues) — an acceptable cost for a dev tool where the UI is forms-and-panes rather than pixel-perfect canvas. Both Yaak and Hoppscotch shipping on Tauri 2 de-risks the choice for exactly this app category.

## 8. Storage + full-text search for local-first history

**SQLite FTS5 is the default answer and fits this workload well.**

- FTS5 virtual tables support incremental updates, BM25 `ORDER BY rank`, phrase/prefix/NEAR queries, highlight/snippet auxiliary functions ([sqlite.org/fts5](https://www.sqlite.org/fts5.html)).
- Use **external-content tables** (`content=history_entries`) so URL/headers/body text isn't stored twice, with triggers keeping the index in sync — the pattern Datasette documents ([Datasette FTS docs](https://docs.datasette.io/en/stable/full_text_search.html)).
- For URLs, tokens like `api.example.com/v2/users` need care: the default unicode61 tokenizer splits on punctuation (often what you want); add the **trigram tokenizer** on a URL column for substring matching (`LIKE`-style but indexed).
- One DB file gives you history + collections metadata + FTS + transactions.
- Follow **Yaak's split**: metadata + searchable text in SQLite; huge response bodies on the filesystem (index only the first N KB of body text).

**Alternatives:**

- **Tantivy** (Rust, Lucene-style): better ranking, faceting, real tokenizer pipeline, faster on large corpora; Turso built its FTS on it to go "beyond FTS5" ([Turso blog](https://turso.tech/blog/beyond-fts5)). Cost: a second on-disk store to keep transactionally consistent with SQLite. Justified only if history reaches millions of entries or faceted/fuzzy search is needed; a plausible v2 upgrade, not a v1 need.
- **DuckDB**: analytics-oriented, experimental FTS extension; wrong fit for a high-write-rate OLTP history log.
- **Precedents:** browser history is SQLite (Chrome `History`, Firefox `places.sqlite`); recall tools like Rewind/screenpipe use SQLite FTS over captured activity. No mainstream API client currently does FTS5 over HTTP history. **The niche is genuinely open.**

## 9. Scripting sandbox (pm.*-style pre-request/test scripts)

**In a Tauri/Rust app:**

- **rquickjs** (bindings to QuickJS-NG): small (~200 KB engine), ES2020+, fast startup, per-context memory/interrupt limits, no ambient fs/network — capabilities only exist if you expose them ([GitHub](https://github.com/DelSkayn/rquickjs)). Best default for untrusted collection scripts.
- **deno_core** (V8): full JS perf and modern APIs, ops-based capability injection; much heavier binary (+V8), more build complexity. Choose if near-Node script compatibility (Postman-import fidelity) is required.
- **Boa** (pure-Rust JS engine): easiest build story, improving conformance, but the slowest and least battle-tested.
- **Yaak's pragmatic third way:** ship a **Node.js sidecar** for plugins/scripts — full npm compatibility, process-level isolation, at the cost of bundling Node (~25 MB) and IPC latency.

**In Electron:**

- **`node:vm` is explicitly not a security boundary** — the Node docs and ecosystem are unambiguous.
- **vm2**: deprecated July 2023 after 8 critical sandbox-escape advisories; brief 2025 revival followed by another critical CVE.
- **isolated-vm**: real V8 isolates, but **in maintenance mode**.
- Industry direction confirms WASM/QuickJS: **Bruno's Safe Mode is QuickJS-in-WASM**; Postman runs scripts through its `postman-sandbox`/`uvm` layer (Node vm / iframes — a trust boundary, not a hard one).

## 10. Bottom line

Tauri 2 + Rust + React; SQLite (rusqlite/sqlx) with FTS5 external-content index over method/URL/headers/body-preview + trigram on URL; response bodies on disk à la Yaak; rquickjs for a capability-scoped `pm.*`-compatible sandbox (optional Node sidecar later for npm-dependent power users). Every incumbent's history is either cloud-gated (Postman), stored in a dead DB (Insomnia/NeDB), non-persistent (Bruno — with open issues begging for this exact feature), or bolted onto a web architecture (Hoppscotch). Yaak proves the architecture but has no FTS story — that's the wedge.
