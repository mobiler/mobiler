---
title: One core for mobile, web and server
description: Lay out a product so its Android and iOS apps, its web app and its Rust server share types, rules and screens.
---

A typical product has a mobile app, a web app and a backend, often written three times in three
languages. With Mobiler they are crates in one Cargo setup, and each piece of code lives in exactly one
of them.

## The layout

```
domain/      types, the API contract, pure rules (no I/O; compiles to WebAssembly)
server/      the backend: Axum, SQLx, whatever you use; depends on domain
app-core/    the MobilerApp: state, logic and screens; depends on domain
mobile/      the Android and iOS app made by `mobiler new`; its shared crate re-exports app-core
web/         the web client: mobiler_web::run::<app_core::App>()
```

| Crate | Used by | Holds |
|---|---|---|
| `domain` | server, app-core (and so mobile and web) | Request and response types, IDs, validation, pricing or scheduling rules, anything both sides must agree on. |
| `app-core` | mobile, web | The `Model`, `Msg`, `update` and `view`: what the user sees and does. |
| `server` | | Storage, auth, the HTTP API. |

## Why it pays off

- **The client and server can't disagree on the wire.** Both serialize the same `domain` types, so a
  renamed field or a new enum case is a compile error on the side that didn't update.
- **Rules run where they're needed.** A validation function in `domain` checks a form instantly in the
  app and again on the server, from one implementation.
- **Mobile and web are one app.** `app-core` renders the same screens on Android, iOS and the web, so a
  feature ships to all three at once and is tested once, with `cargo test`.
- **Your web app can still be its own.** If the web needs a different UI (a dense admin table, a
  marketing page), write it as a normal Leptos (or any) web app that depends only on `domain`. It talks
  the same JSON as the mobile app.

## Talking to the server

The core calls the server through the HTTP capability. The shell runs the request with the platform's own
HTTP stack and hands the result back as a message:

```rust
// app-core: the continuation turns the outcome into one of your messages
fn load(cx: &mut Cx<Msg>) {
    cx.get(todos_url(), |resp| parse_list(&resp));
}

fn parse_list(resp: &HttpOutcome) -> Msg {
    if !resp.is_success() {
        let status = resp.status().map_or("no answer".to_string(), |s| s.to_string());
        return Msg::Failed(format!("server returned {status}"));
    }
    match serde_json::from_str::<Vec<domain::Todo>>(resp.text().unwrap_or_default()) {
        Ok(todos) => Msg::Loaded(todos),
        Err(e) => Msg::Failed(format!("parse error: {e}")),
    }
}
```

```rust
// server (Axum): the same domain::Todo, serialized by the other side
async fn list(State(s): State<AppState>) -> Json<Vec<domain::Todo>> {
    Json(s.lock().unwrap().todos.clone())
}
```

This is the [full-stack todo demo](https://github.com/mobiler/mobiler/tree/main/demos/fullstack-todo)'s code,
shortened. Its `err_of` also tells a network error from a server error.

Keep the API plain JSON over HTTP, so the mobile core, a separate web app and any other client are equal
users of the same endpoints.

During development the app reaches a server on your computer at `http://10.0.2.2:3000` from the Android
emulator, and at `http://localhost:3000` from the iOS simulator and the browser. iOS blocks plain HTTP by
default; allow local addresses with `[ios] local_network = true` in `mobiler.toml` (see
[mobiler.toml options](/guides/configuration/#local-network-on-ios)).

## Examples

- [Full-stack todo](/examples/#full-stack-todo): `domain`, an Axum server, one core on Android, iOS and the
  web, plus a separate Leptos web app that shares only `domain`.
- [Full-stack notes with SQLx](/examples/#full-stack-notes-sqlx): the same shape with a real database.
- [Barbershop](/examples/#barbershop-fade-house): `app-core` shared by the mobile app and the web client.
