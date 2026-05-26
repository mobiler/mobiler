# Full-stack todo — Mobiler demo

One all-Rust product across surfaces, showing **how much you can reuse** between
mobile and web. A flat todo list backed by one Axum server (in-memory, no DB),
with the data model + API contract shared by every client via the `domain` crate.

The **same `todo-core`** (Rust logic + `view`→`Widget`) renders natively on Android
**and** as a web app — both reading the same server. Same data, platform-appropriate
look:

<p>
  <img src="screenshots/mobile.png" width="250" alt="Native Android (Compose)">
  &nbsp;&nbsp;
  <img src="screenshots/web.png" width="380" alt="Web (Widget→DOM via Leptos)">
</p>

It deliberately demonstrates **two reuse strategies**:

| Client | Reuses | Point |
|---|---|---|
| `mobile/` | the whole `todo-core` (logic + `view`→`Widget`) | native Mobiler app |
| `web-widgets/` | the **same** `todo-core`, compiled to WASM | web is just *another shell* rendering the same `Widget` tree to DOM — the Mobiler superpower (essentially `mobiler-web`) |
| `web-json/` | only `domain` (types/intents) | an idiomatic Leptos app with its own UI, talking JSON (planned) |

The API is **plain JSON REST** (not Leptos server functions) so the Mobiler core and
the web app are equal clients of the same endpoints.

## Layout

```
domain/       shared types + intents + pure logic (WASM-clean)        ✅
server/       Axum + in-memory store, JSON REST                       ✅
todo-core/    the MobilerApp (server-backed via the HTTP capability)  ✅
mobile/       native shell over todo-core/                            ✅
web-widgets/  Widget→DOM shell hosting todo-core as WASM (Leptos)     ✅
web-json/     own Leptos components, fetch JSON (shares domain only)  ⬜ planned
```

## Run

```bash
# 1) the backend (both clients point at it)
cargo run -p server                 # http://0.0.0.0:3000

# 2a) the native app — from mobile/
cd mobile && mobiler dev            # builds + launches on a device/emulator
#     (mobile reaches the host server at http://10.0.2.2:3000)

# 2b) the web app — from web-widgets/
cd web-widgets && trunk serve       # http://localhost:8080
#     (web reaches the server at http://localhost:3000)
```

Add/toggle/delete on either client; the change round-trips through the server and
shows up on the other after its next load. The "N left" badge is computed by
`domain::active_count` — shared pure logic, identical on every surface.
