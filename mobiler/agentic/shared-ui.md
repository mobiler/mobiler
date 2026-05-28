
---

## Architecture for THIS app: the same UI on mobile and web

This app renders **one `Widget` tree on every platform** — mobile and web look identical.

- **Mobile**: `shared/` (this scaffold) + the generic Android/iOS shells.
- **Web**: a tiny Trunk crate whose `main` is `mobiler_web::run::<shared::App>()` — it renders the
  *same* core, so the web UI is automatically identical to mobile. Add it by hand (it's one line
  + a minimal `index.html`); see `demos/coffee/web` for the exact shape.
- **Data**: keep it local (`cx.save` / `restore`) for a simple app, or talk to a backend over
  `cx.http`. If you need a server + database, follow `demos/fullstack-sqlx` (Axum + SQLx).

**If the user has a web design reference or prototype** (e.g. a Lovable/Figma export, or a
prototype running locally): **ask for its path on disk, read it**, and **translate** the
screens / layout / flow into Mobiler's `Widget` tree + theme. Do NOT copy the HTML/React/CSS —
re-express the same UX with the builders. If a design needs a widget Mobiler lacks, pick the
closest builder and note the gap (don't invent ABI widgets).

**Do not write a separate web UI** — the whole point of this flavor is one UI everywhere.
