
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

**Before building, ask the user** for the backend base URL and the app's entities / screens.
