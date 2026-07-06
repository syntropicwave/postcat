# postcat-sync

Self-hostable, **end-to-end-encrypted** sync server for [postcat](../).

The server is deliberately blind: it stores only ciphertext blobs and verifies
logins by comparing a SHA-256 of a client-derived auth verifier. The password
never leaves the device, so neither our hosted instance nor a company's own
deployment can read collections, environments or history. A lost password is
recoverable only via the one-time recovery code shown at sign-up.

This covers personal E2E sync only. Server-managed team secrets (where clients
use a value without seeing it) are a separate, non-E2E subsystem — see
[../docs/sync-design.md](../docs/sync-design.md).

## Run

```sh
# from source
POSTCAT_SYNC_DB=./sync.db POSTCAT_SYNC_ADDR=0.0.0.0:8787 cargo run --release

# docker
docker build -t postcat-sync .
docker run -p 8787:8787 -v postcat-sync-data:/data postcat-sync
```

Then in postcat: Settings → Sync → server URL `https://your-host:8787`. Put a
TLS-terminating reverse proxy in front for production.

| Env var | Default | Meaning |
|---|---|---|
| `POSTCAT_SYNC_DB` | `postcat-sync.db` | SQLite database path |
| `POSTCAT_SYNC_ADDR` | `0.0.0.0:8787` | listen address |

## API

All bodies are JSON. Sync endpoints need `Authorization: Bearer <token>`.

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | liveness |
| POST | `/v1/register` | `{ email, blob }` — create account (409 if taken) |
| GET | `/v1/salt?email=` | public password salt (needed to derive keys) |
| POST | `/v1/login` | `{ email, auth_verifier }` → `{ token, wrapped_by_password }` |
| GET | `/v1/recover-info?email=` | `{ recovery_salt, wrapped_by_recovery }` |
| POST | `/v1/push` | `{ blobs: [...] }` — upsert, last-writer-wins on `rev` |
| GET | `/v1/pull?since=<cursor>` | items changed after the server cursor |

`blob` and `blobs[]` carry only opaque `ciphertext` plus routing metadata
(`kind`, `item_id`, `rev`, `updated_at`, `deleted`). The server assigns a
monotonic `seq` used as the pull cursor.
