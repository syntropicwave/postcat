# Postman Feature Inventory — Research Summary

> Research date: 2026-07-06. Compiled from Postman's official Learning Center docs, Postman blog posts, GitHub issue tracker, community forum threads, and third-party comparisons. This is the target feature spec reference for postcat.

---

## 1. Core Request Building (Core)

**HTTP methods:** GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, plus arbitrary custom methods (free-text method field).

**URL & params:**
- URL bar with automatic parsing of query params into a key-value **Params** table (bidirectional sync: edit table or URL).
- **Path variables** (`:id` segments) get their own rows in the params table.
- Variables (`{{baseUrl}}`) usable anywhere in URL, params, headers, body, with autocomplete and hover-to-see-value.
- Bulk-edit mode (raw key:value text) for params and headers.
- Per-row enable/disable checkboxes and per-row descriptions.

**Headers:** key-value editor with autocomplete of standard header names, auto-generated headers (Host, Content-Length, auth headers) shown in a collapsible "hidden headers" section; header presets (saved reusable header groups).

**Body editors:**
- `none`
- `form-data` — key-value with text or **file** values; per-row content-type override
- `x-www-form-urlencoded`
- `raw` — with syntax dropdown: Text, JavaScript, JSON, HTML, XML (sets Content-Type + syntax highlighting, JSON linting, beautify button)
- `binary` — single file upload
- `GraphQL` — query + variables panes

**Cookies:** a **Cookie Manager** (per-domain jar) to view/create/edit/delete cookies manually; automatic cookie capture/persistence across requests; cookie sync from browsers via Interceptor/proxy for whitelisted domains; programmatic access via `pm.cookies` and `pm.cookies.jar()` in scripts (domain allowlist required for jar access).

Other core request features: request renaming/description, saving requests to collections, tabbed multi-request UI, keyboard shortcuts, "Code" pane for snippet generation, per-request settings overrides (SSL verification, redirects, max redirect count).

## 2. Authentication Helpers (Core, with some advanced types)

Auth is set on the **Authorization tab**; Postman auto-injects the right headers/params. Full list:

| Type | Notes |
|---|---|
| No Auth / Inherit from parent | Inheritance from folder/collection is the default — a key design feature |
| API Key | key/value, placed in header **or** query param |
| Bearer Token | `Authorization: Bearer <token>` |
| JWT Bearer | Generates JWT from configurable algorithm/secret/payload in-app |
| Basic Auth | username/password → Base64 |
| Digest Auth | with auto-retry on 401 challenge |
| OAuth 1.0 | signature methods incl. HMAC-SHA1/256, adds to header or body/URL |
| OAuth 2.0 | Full token-acquisition UI: grant types = Authorization Code (**with PKCE**), Implicit, Password Credentials, Client Credentials; opens browser or embedded window for consent; **token management** (stores, names, refreshes tokens; auto-refresh support); configurable client auth (body vs Basic header) |
| Hawk | partial cryptographic verification |
| AWS Signature (SigV4) | AccessKey/SecretKey/Region/Service, session token support |
| NTLM (Windows) | challenge-response, v1/v2 |
| Akamai EdgeGrid | vendor-specific helper |
| ASAP (Atlassian S2S) | JWT bearer variant |

Advanced/cloud-tied: "Guided Auth" for public APIs, team-shared token vaults. The per-request helpers above are all local/core.

## 3. Response Viewing (Core)

- **Body views: Pretty / Raw / Preview / Visualize.** Pretty = formatted + syntax-highlighted JSON/XML/HTML with collapsible nodes and clickable links (click loads a GET in a new tab). Preview = rendered HTML sandbox. Visualize = user-programmable rendering via Handlebars templates set in test scripts (`pm.visualizer.set`) — an advanced but loved feature.
- **Search in response:** search icon or Ctrl/Cmd+F inside the response pane.
- **Metadata:** status code (with hover explanation), **response time**, **response size** (body + headers breakdown on hover), and a network info popover (IP, TLS version, certificate details).
- Headers tab, Cookies tab, Test Results tab per response.
- **Save response:** "Save as Example" (attaches example to the saved request — used by mocks/docs) or "Save response to file."
- Console (Postman Console) shows raw wire-level request/response logs, script `console.log`, errors — essential debugging surface.

## 4. History (Core — and Postman's weak spot)

**How it works:**
- Sidebar **History tab** logs every sent request, grouped by date.
- Stores: method, URL, params, headers, body, auth of the request. **Responses are NOT saved by default** — user must toggle "Save Responses" in History options.
- Collection runs are stored as summarized run entries, not individual requests.
- When signed in, history **syncs to Postman cloud** across devices; it is private to the user even in shared workspaces.
- Actions: search bar, add request back to a collection ("+"), multi-select (Ctrl/Cmd+click) for bulk delete/save, delete single item, Clear All.

**Known limitations & user complaints (postcat's differentiation target):**
- **Lazy loading breaks search:** Postman loads only a few days of history at a time; the search box only searches *loaded* entries, so finding a request from a month ago requires minutes of incremental scrolling, and the loaded window resets on restart ([GitHub #9566](https://github.com/postmanlabs/postman-app-support/issues/9566), [#9513](https://github.com/postmanlabs/postman-app-support/issues/9513)).
- **Search is URL/name-only.** Users explicitly ask for (and don't get) **search by request body content and by date range** ([community thread](https://community.postman.com/t/is-there-any-way-to-search-the-request-body-in-history-or-search-by-specific-date/7784)).
- No filter by method, status code, host, or workspace; no diffing between history entries.
- **Retention tied to plan/login:** on sign-out only the last ~10 requests survive locally (100 on paid tiers per community reports).
- Reliability bugs: "Could not find the history you are looking for" for older entries ([#11764](https://github.com/postmanlabs/postman-app-support/issues/11764)); POST bodies missing from history entries; entire history vanishing after version upgrades ([community](https://community.postman.com/t/postman-8-0-6-cant-remember-history/21100)).
- Responses off-by-default means the most useful half of a history record usually isn't there.

**Opportunity summary:** local full-text index over URL + headers + request body + response body, date-range and method/status/host filters, unlimited retention, responses always stored, instant offline search. Every one of these addresses a documented complaint.

## 5. Collections (Core)

- **Structure:** Collection → nested **folders** (arbitrary depth) → requests → **saved examples** (request+response pairs). Collection and folder levels each carry: description (Markdown), auth (inherited downward), pre-request & test scripts (run for all children), and **collection variables**.
- **Import:** Postman Collection v2.0/v2.1 JSON (v1 no longer supported), **cURL** commands (paste into URL bar or import dialog), **OpenAPI 1/2/3.0/3.1**, Swagger, RAML, WADL, WSDL, GraphQL schemas, HAR-style captures via proxy, and migrations from Insomnia, SoapUI, Hoppscotch, Thunder Client.
- **Export:** Collection v2.0/v2.1 JSON (self-contained file); environments and globals export as JSON too. Collection Format is open-spec (schema.postman.com).
- **Collection Runner:** runs a collection/folder in sequence; **iterations** count, per-request **delay**, **data files (CSV/JSON)** feeding `data` variables per iteration, run order editing, skip requests, workflow control via `pm.execution.setNextRequest()`, aggregated test results and run reports. Advanced/paid: scheduled cloud runs, parallel runs, performance testing. CLI equivalents: **Newman** (open-source) and **Postman CLI**.
- Advanced/cloud: forking/merging collections with version control, comments, collection-level "watch," monitors built from collections.

## 6. Environments & Variables (Core; secrets partly advanced)

- **Scopes (precedence narrow→broad): local (script/run-scoped) > data (runner CSV/JSON) > environment > collection > global.**
- Environments are named variable sets; one active at a time; quick-switcher; each variable has **initial value** (synced/shared) vs **current value** (local-only, never synced) — an important privacy design.
- Script access: `pm.environment`, `pm.globals`, `pm.collectionVariables`, `pm.variables` (scope-resolving), `pm.iterationData`.
- **Dynamic variables:** `{{$guid}}`, `{{$timestamp}}`, `{{$randomInt}}`, plus a large Faker-backed set (`$randomFirstName`, `$randomEmail`, `$randomCity`, ~100 more); usable in scripts via `pm.variables.replaceIn()`.
- **Secret variable type:** masks value in UI for all workspace members; newer "vault"/secure variables use AES-256 encryption locally. Known leaks reported: secrets still visible in console logs, Newman HTML reports, and resolved headers ([GitHub #10654](https://github.com/postmanlabs/postman-app-support/issues/10654), [#10650](https://github.com/postmanlabs/postman-app-support/issues/10650), [#10906](https://github.com/postmanlabs/postman-app-support/issues/10906)) — another differentiation angle.
- Advanced/cloud: Postman Vault (local encrypted store with cloud-provider secret integrations), environment sharing/roles.

## 7. Scripting (Core)

- **Pre-request scripts** and **Post-response (test) scripts** at request, folder, and collection level (outer scripts run first). Written in JavaScript, executed in the **Postman Sandbox** (Node.js-based isolated runtime; also used by Newman/CLI).
- **`pm.*` API:** `pm.request` (mutate URL/headers/body pre-send), `pm.response` (`.json()`, `.text()`, `.code`, `.status`, `.headers`, `.responseTime`, `.to.have.*` assertion sugar), `pm.test(name, fn)` + `pm.expect` (**Chai.js BDD assertions built in**), `pm.sendRequest()` (async ancillary requests), variable objects, `pm.cookies`, `pm.visualizer`, `pm.execution.setNextRequest()`/`pm.execution.skipRequest()`, `pm.info` (iteration metadata), `pm.require()` for **external/npm packages** (Package Library — newer, partially cloud/paid).
- Built-in libraries: Chai, cheerio, lodash, moment, crypto-js, uuid, xml2js, ajv, atob/btoa.
- Snippet sidebar with ready-made test snippets (status code check, JSON value check, schema validation, etc.).
- gRPC/WebSocket have analogous script hooks (Before invoke / On message / After response).

## 8. Protocol Support Beyond HTTP

| Protocol | Support level |
|---|---|
| **GraphQL** | Dedicated GraphQL client: auto **introspection**, schema explorer, click-to-build visual query builder, variables pane, multi-query selection; also usable as plain HTTP body type. Core. |
| **WebSocket** | Raw WS requests: connect, send/receive messages (text/JSON/XML/binary views), saved messages, headers/params on handshake, reconnect settings, message search. Core (desktop app only). |
| **Socket.IO** | First-class wrapper: event-based emit with args, **listen to named events**, acknowledgements, v2/v3/v4 client selection. Core (desktop only). |
| **gRPC** | Import `.proto` / server reflection, unary + client/server/bidi **streaming**, metadata, TLS, message templates, assertions via scripts. Core (desktop only). |
| **MQTT** | v3.1.1 and v5: connect to broker, subscribe to topics, publish, QoS, wills, TLS/self-signed certs, real-time visualization of received messages. Newer, beta-ish. |
| **SSE** | Handled through a normal HTTP request; Postman keeps the connection open and streams events into the response pane. Core. |
| **SOAP** | Via raw XML HTTP requests + WSDL import (no dedicated client). |
| gRPC-Web, MCP | MCP (Model Context Protocol) client added recently in the request builder. |

Note: non-HTTP protocols mostly require login/workspace and the desktop app (not the web client).

## 9. Other Notable Features

- **Mock servers** — built from a collection's saved examples; matches incoming calls to examples, dynamic responses using request data, simulated latency/errors/rate-limits. **Cloud-hosted only** (mock URL lives on mock.pstmn.io); free tier has monthly call limits.
- **API documentation** — auto-generated from collection descriptions + examples; Markdown; multi-language sample code; publishable web docs with "Run in Postman" button. Generation is cloud-tied for publishing.
- **Code snippet generation** — per-request "Code" icon; generators (open-source, `postman-code-generators`) for: cURL, HTTP raw, JavaScript (fetch/axios/jQuery), Node.js (native/axios/request), Python (requests/http.client), Go, Java (OkHttp/Unirest), C# (HttpClient/RestSharp), PHP, Ruby, Swift, Objective-C, Kotlin, Rust, R, PowerShell, wget, Dart, Shell/Httpie. Fully local. **This list is a good target spec for a competitor.**
- **Proxy / traffic capture** — built-in HTTP proxy (point any client/phone at it) and browser **Interceptor** extension; captures requests + cookies into history/collections, with method/URL filters. Local.
- **Certificates** — global Settings > Certificates: custom **CA certs** (PEM, multiple) and **client certificates** (CRT/KEY or PFX + passphrase) mapped to host patterns with wildcard support (`*.example.com`). Local.
- **Settings** — SSL certificate verification toggle (global + per-request), follow redirects (+ max count, follow original HTTP method, retain auth on redirect), request timeout (ms, 0 = infinite), max response size, proxy (system default / custom, with auth and bypass list), send no-cache & Postman-token headers, language detection, two-pane vs single-pane layout, themes, font size, autosave, working directory for file paths, telemetry toggle.
- Also in the product (mostly cloud/paid): Monitors (scheduled cloud runs), API Builder (spec-first design), workspaces (personal/team/public), Flows (visual low-code), Postman AI ("Postbot"), governance/security scanning, Live Collections/API Network.

## 10. Cloud vs Local — and What Users Criticize (Positioning Input)

**Requires cloud/account:** collections & environments (in the standard signed-in mode everything syncs to Postman's servers), mock servers, monitors, published docs, team workspaces, Flows, version control/forking, Postbot, history sync. Free tier caps mock/monitor calls and collaborators.

**Works locally:** the request engine itself, scripting sandbox, code generators, proxy/interceptor capture, certificates, Newman CLI runs against exported JSON.

**Major criticisms (well documented):**
1. **Forced login / Scratch Pad removal (Sept 2023).** Offline Scratch Pad was killed; opening collections now requires a cloud account. The replacement "**lightweight API client**" (no-login mode) is deliberately crippled: requests + history only — **no collections, no environments/variables, no tests, no import/export of saved work** ([Postman blog](https://blog.postman.com/announcing-new-lightweight-postman-api-client/), [community backlash](https://community.postman.com/t/postman-stopped-supporting-scratch-pad-what-next/52530), [v10 offline thread](https://community.postman.com/t/postman-version-10-no-more-offline-mode-renders-postman-useless-for-many-developers/48812)).
2. **Data residency/security concerns** — all collections (often containing tokens, internal URLs) sync to Postman's cloud; many enterprises banned it after the 2023 change.
3. **Bloat & performance** — Electron app with heavy startup bundles, high memory/CPU, sluggishness during collection runs; long-running GitHub issues ([#7294](https://github.com/postmanlabs/postman-app-support/issues/7294), [macOS 26 GPU issue #13836](https://github.com/postmanlabs/postman-app-support/issues/13836); [HN: "so incredibly bloated and slow"](https://news.ycombinator.com/item?id=30177337)).
4. **Feature creep** — API design, governance, AI, marketplace crowd out the core client; "all I want is a REST client" sentiment.
5. **Pricing/deprecations** — paywalling of previously-free capabilities, free-tier limits, killed features.
6. **History weaknesses** (§4) — poor search, retention tied to plan, sync-dependent.

Competitors positioned on exactly these gaps: **Bruno** (git-native plain-file collections, offline, no account), **Insomnia** (familiar UX, though it also had a forced-account backlash in 2023), **Hoppscotch** (open-source/self-host), **Yaak**, **Thunder Client**. A local-first desktop app with **best-in-class searchable history** (full-text over bodies/responses, date filters, unlimited local retention) attacks an area where none of the majors — including Postman — is strong.
