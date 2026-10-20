---
title: What's shared
description: What you write once in Rust, what Mobiler provides for each platform, and how a product with mobile apps, a web app and a server reuses one codebase.
---

A Mobiler app is one Rust **core** plus a thin **shell** per platform. You write the core. The shells are
generic: the same Android, iOS and web shells render every Mobiler app, and the `mobiler` CLI writes and
updates them for you.

## Written once, in Rust

Everything an app *decides* lives in the core and runs unchanged on Android, iOS and the web.

| What | How it looks in the core |
|---|---|
| State | Your `Model` struct. |
| Business rules and validation | Plain Rust functions, called from `update`. |
| What happens on a tap or a keystroke | Your `Msg` enum and `update(msg, model, cx)`. |
| The screens | `view(model) -> Widget`: rows, cards, lists, forms, tabs, sheets, charts, maps, built from widget functions. |
| Navigation | A `Nav` stack the core owns; the shells animate push and pop and handle the system back button. |
| Look and brand | A `Theme` value: seed colour or a full light/dark palette, corners, fonts, density. |
| Translations | `mobiler_core::i18n`: language negotiation and a catalogue with fallbacks. |
| Numbers, money and dates | `mobiler_core::format`, per locale. |
| Network calls | `cx.get`, `cx.post`, `cx.request`, streaming upload and download. |
| Device features | `cx.*` capabilities and plugins: storage, camera, photos, clipboard, haptics, SQLite, biometrics, push, and [more](/reference/capabilities/). |
| Tests | `cargo test` on your machine: the core is a pure function of messages, so a test sends messages and checks the model and the screen it returns. |

Because the core is ordinary Rust, any of it can come from crates you already have: your API types, your
pricing rules, your date logic.

## Provided per platform

These parts exist once per platform, are the same for every app, and are maintained by Mobiler, not by you.

| Part | Android | iOS | Web |
|---|---|---|---|
| The shell that draws the widget tree | Jetpack Compose, Material 3 | SwiftUI | The DOM, via Leptos and WebAssembly |
| Built-in capabilities (HTTP, storage, camera…) | Kotlin | Swift | Browser APIs |
| Plugins you add | Kotlin, copied into your app | Swift, copied into your app | Not on the web yet: a plugin call answers "not available", so the core can show something else |
| Where it comes from | `mobiler new`, updated by `mobiler upgrade` | `mobiler new`, updated by `mobiler upgrade` | The `mobiler-web` crate |

What stays yours on each platform is configuration, not code: the app icon, the name users see, the
package id, store signing, and options in `mobiler.toml` such as the launch screen and deep links.

The shell never sees your app's types. The core sends it a fixed widget vocabulary and receives taps and
text back as opaque tokens, so one prebuilt shell can render any app, and your app never needs a line of
Kotlin, Swift or JavaScript.

## One product: mobile, web and a server

Most real products have more than one client and a backend. Mobiler is built for that shape, and the
pieces are ordinary Cargo crates:

```
domain/      your types, API contract and pure rules: used by everyone, compiles to WebAssembly
server/      an Axum (or any Rust) server that uses domain
app-core/    the MobilerApp: logic and screens, uses domain, calls the server with cx.get/post
mobile/      the Android and iOS app: its shared crate re-exports app-core (CLI-managed shells)
web/         the web client: mobiler_web::run::<app_core::App>()
```

- The **mobile app and the web client run the same `app-core`**, so they show the same screens and
  follow the same rules. The web client is one line of Rust.
- The **server and the clients share `domain`**, so a request the client builds is a type the server
  parses. A renamed field breaks the build, not production.
- You can also give the web its own UI: a web app that shares only `domain` and draws its own Leptos
  components still talks the same JSON. The [full-stack todo demo](/examples/#full-stack-todo) runs
  both side by side.

The [full-stack guide](/guides/full-stack/) shows how to set this up.

## What isn't the same everywhere

- **Native look.** Each shell follows its platform: a Material switch on Android, an iOS toggle on iOS.
  The widget tree is the same; the pixels are not, by design.
- **Platform-only options.** A few options exist on one platform only and are ignored elsewhere, for
  example iOS's floating glass tab bar.
- **Plugins are mobile-only for now.** Built-in capabilities work in the browser, some with limits (a page
  can't mark a copied text sensitive), but plugins such as SQLite or biometrics run only on Android and
  iOS. A web client that needs them gets a "not available" answer it can handle, or talks to your server
  instead.
- **The web redraws.** The web shell rebuilds the page on every update. It keeps each tab's scroll
  position, but not a half-typed field on a tab you left, which the native shells keep.
- **Store work.** Signing, store listings and review are per platform, as with any app.
