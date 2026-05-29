
---

## Architecture for THIS app: reusable core + JSON API

This app has a **backend** (a JSON API over HTTP) and a **shared core**, so business rules live
in Rust once and every client agrees.

```
domain/    serde types + business rules — the single source of truth (used by all of the below)
server/    Axum + SQLx (SQLite in dev → PostgreSQL in prod) JSON API over `domain`
shared/    the Mobiler mobile core (this scaffold) — talks to the API via cx.http
web/       the web client — EITHER `mobiler_web::run::<App>()` (same UI as mobile)
           OR a bespoke Leptos app (its own richer/desktop UX), sharing `domain`
```

- Model the entities + rules in **`domain/`** first (with tests). The server and every client
  depend on it; never duplicate a business rule in a UI layer.
- Build the **`server/`** on **Axum + SQLx** — see `demos/fullstack-sqlx` for a working reference
  (runtime queries, so no `DATABASE_URL` / `sqlx prepare`; SQLite → Postgres by swapping the SQLx
  feature, the connection URL, and the table DDL; use `sqlx migrate` for real schemas).
- The Mobiler app (`shared/`) fetches via `cx.get` / `post` / `patch` / `delete` — it **never**
  touches the database directly.
- For the **web**: if it should look like mobile, use the one-line `mobiler-web` shell; if it
  needs a different, richer experience, write a **bespoke Leptos app** that consumes `domain` +
  the same API (presentation only — no business logic in the web layer).

### Production database — SQLite by default

SQLite is the right default for a small app (one shop/venue, modest traffic): it handles
thousands of writes/sec, while such an app does a handful per minute. **When you build the
server's DB setup, always:**

1. **Set the production pragmas** on the SQLite pool (not optional for a web server):
   - `journal_mode = WAL` — readers don't block the writer (and required by Litestream).
   - `busy_timeout` (~5s) — a brief write lock waits instead of erroring `database is locked`
     (SQLite serializes writes; this is the #1 gotcha if you skip it).
   - `synchronous = NORMAL`, `foreign_keys = ON`. Build the pool from `SqliteConnectOptions`
     with `create_if_missing(true)`; a small `max_connections` is plenty (writes serialize;
     WAL lets reads run concurrently). Copy `demos/fullstack-sqlx` `server/src/main.rs` `connect()`.
2. **Add a backup** — use **Litestream** (`litestream replicate -exec "./server"`): continuous
   replication of the DB file to S3/R2/B2 with point-in-time restore. A SQLite app without a
   backup story is not production-ready. (See `demos/fullstack-sqlx/server/litestream.yml`.)
3. **Shut down gracefully** (close the pool on SIGTERM/Ctrl-C → checkpoints the WAL), log with
   `tracing`, and make the bind address configurable (`0.0.0.0` in a container).

**Use PostgreSQL instead when:** multiple app instances share one DB (SQLite is a local file —
don't share it over a network FS), a shared multi-tenant DB grows large, or you want managed
HA/backups (RDS/Supabase/Neon/Fly). Thanks to **runtime SQLx queries**, switching is mostly a
connection-string + DDL change — start on SQLite, migrate only if a client outgrows it. Use a
`migrations/` dir + `sqlx migrate` for real schemas (not inline `CREATE TABLE`).

**Before building, ask the user** for the backend base URL and the app's entities / screens.
