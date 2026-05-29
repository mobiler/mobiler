# fullstack-sqlx

A fullstack Mobiler demo with a **real database**: one shared Rust core renders on
mobile + web, backed by an **Axum + SQLx** JSON API persisting to **SQLite** (movable to
PostgreSQL in production). A small "notes" app — list / add / delete.

```
domain/     serde types shared by everyone (the wire contract)
server/     Axum + SQLx (SQLite) JSON API: GET/POST /notes, GET/DELETE /notes/{id}
app-core/   the Mobiler app (logic + UI in Rust) — talks to the server via cx.http
web/        web client: `mobiler_web::run::<app_core::App>()` (renders app-core)
```

## Run it

**1. Server** (creates `notes.db` on first run):
```bash
cd server && cargo run        # → http://127.0.0.1:3000
```

**2. Web client:**
```bash
cd web && trunk serve         # open the printed URL
```

**3. Mobile** (same `app-core`): scaffold a Mobiler app and point its core at `app-core`:
```bash
mobiler new notes-mobile
# then make notes-mobile/shared re-export app-core's `App`
# (add `app-core = { path = "../app-core" }` and `pub use app_core::App;`),
# regenerate types, and `mobiler dev`.
```

## Notes on the stack

- **SQLx runtime queries** (`sqlx::query` / `query_as`) are used instead of the compile-time
  macros, so the server builds with **no `DATABASE_URL`** and **no `sqlx prepare`** step. If you
  switch to the macros for compile-time-checked SQL, run `cargo sqlx prepare` and commit `.sqlx/`.
- **SQLite → PostgreSQL:** change the SQLx feature (`sqlite` → `postgres`), the connection URL,
  and the `CREATE TABLE` DDL (`INTEGER PRIMARY KEY AUTOINCREMENT` → `BIGSERIAL PRIMARY KEY`).
  The handlers and `domain` types don't change. A real app would use a `migrations/` dir +
  `sqlx migrate` rather than the inline `CREATE TABLE IF NOT EXISTS` here.
- **CORS** is permissive on the server so the web client (a different origin) can call it.
- The app talks to the server **only over HTTP** (`cx.get/post/delete`) — never a direct DB
  connection from a client.

## Production (SQLite is a great fit for small apps)

SQLite runs small production apps just fine — a coffee-shop/restaurant ordering app does a
handful of writes per *minute*; SQLite handles thousands per *second*. The server is already
set up for it (`server/src/main.rs`, `connect()`):

- **WAL journal mode** — readers never block the writer (and vice versa); essential for a web
  server, and required by Litestream.
- **`busy_timeout`** — a brief write lock *waits* instead of erroring with the infamous
  `database is locked` (SQLite serializes writes — this pragma is the #1 web-server gotcha).
- **`synchronous = NORMAL`** (durable under WAL, fast) and **foreign keys enforced**.
- Small connection pool (writes serialize at the DB level; WAL lets reads run concurrently),
  **graceful shutdown** (Ctrl-C / SIGTERM closes the pool → checkpoints the WAL), structured
  **`tracing`** logs, and a configurable bind (`BIND_ADDR`, default `127.0.0.1:3000` — set
  `0.0.0.0:3000` in a container). Override the DB path with `DATABASE_URL`.

**Backups — use [Litestream](https://litestream.io)** (`server/litestream.yml`): continuous
streaming replication of the SQLite file to S3-compatible storage (R2/B2/MinIO/S3) with
point-in-time restore. Run it as the server's supervisor so it restores on boot and replicates
live:

```bash
litestream replicate -config litestream.yml -exec "./server"
```

**When to reach for PostgreSQL instead:** multiple app instances behind a load balancer sharing
one DB, or a single shared multi-tenant DB at scale, or you want managed HA/backups (RDS,
Supabase, Neon, Fly Postgres). For one venue per instance, SQLite + Litestream is simpler and
cheaper. Because the server uses **runtime SQLx queries**, moving is mostly a connection-string +
DDL change (see the SQLite → PostgreSQL note above) — start on SQLite, migrate only if a client
outgrows it.
