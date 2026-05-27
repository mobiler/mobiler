# mobiler-web

> Mobiler's web shell — the same Rust core, rendered to the DOM.

The web twin of Mobiler's generic Android (Jetpack Compose) and iOS (SwiftUI)
shells. It drives any [`MobilerApp`](https://crates.io/crates/mobiler-core)'s core
with [Crux](https://github.com/redbadger/crux), renders its `Widget` tree to the DOM
with [Leptos](https://leptos.dev) (CSR/WASM), and fulfils the `http` capability with
the browser's `fetch`. Write your app once; it runs on the web with one line.

```rust
// src/main.rs of a Trunk project
fn main() {
    mobiler_web::run::<my_app::App>();
}
```

```html
<!-- index.html — that's all the markup you need -->
<!DOCTYPE html>
<html>
  <head><link data-trunk rel="rust" /></head>
  <body></body>
</html>
```

- **No CSS required.** The shell ships its own theme (`mobiler.css`) and injects it
  on mount, so `run::<App>()` renders a fully-styled app. It's injected at the front
  of `<head>`, so your own stylesheet (if you add one) overrides any widget class.
- **Theme-as-data.** `Scaffold.dark_mode` flips the whole theme — the web twin of
  the native shells' `preferredColorScheme` / Material theme.
- **Complete widget coverage.** Every [`mobiler-ui`](https://crates.io/crates/mobiler-ui)
  `Widget` variant renders (the `match` is exhaustive, like the native shells), so a
  core that runs on Android/iOS renders identically on the web.

Build and serve with [Trunk](https://trunkrs.dev): `trunk serve`. See the
[`fullstack-todo`](https://github.com/mobiler/mobiler/tree/main/demos/fullstack-todo)
and [`coffee`](https://github.com/mobiler/mobiler/tree/main/demos/coffee) demos for
working examples (one core, three platforms).

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
