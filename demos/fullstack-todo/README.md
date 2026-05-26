# Full-stack todo — Mobiler demo

One all-Rust product across surfaces, showing **how much you can reuse** between
mobile and web. A flat todo list backed by one Axum server (in-memory, no DB),
with the data model + API contract shared by every client via the `domain` crate.

It deliberately demonstrates **two reuse strategies** side by side:

| Client | Reuses | Point |
|---|---|---|
| `mobile/` | the whole `core/` (logic + `view`→`Widget`) | native Mobiler app |
| `web-widgets/` | the **same** `core/`, compiled to WASM | web is just *another shell* rendering the same `Widget` tree to DOM — the Mobiler superpower (this is essentially `mobiler-web`) |
| `web-json/` | only `domain` (types/intents) | an idiomatic Leptos app with its own UI, talking JSON |

The API is **plain JSON REST** (not Leptos server functions) so the Mobiler core
and the web app are equal clients of the same endpoints.

## Layout

```
domain/       shared types + intents + pure logic (WASM-clean)
server/       Axum + in-memory store, JSON REST          ✅ built
core/         the MobilerApp (server-backed via the HTTP capability)   ⬜ next
mobile/       native shell over core/                                  ⬜
web-widgets/  Widget→DOM renderer hosting core/ as WASM                ⬜
web-json/     own Leptos components, fetch JSON (shares domain only)   ⬜ later
```

## Run the server

```bash
cargo run -p server      # http://0.0.0.0:3000
# GET/POST /todos, PATCH/DELETE /todos/{id}
```

> Status: foundation only (`domain` + `server`). The HTTP capability, core, and
> clients are being built incrementally.
