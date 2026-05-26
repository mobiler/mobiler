# mobiler-ui

> Mobiler's fixed UI wire ABI.

The stable contract between a [Mobiler](https://github.com/mobiler/mobiler) app's
Rust core and its native shell:

- an app-agnostic **`Widget`** tree (the ViewModel the core emits),
- an **`Action`** protocol (events the shell sends back),
- `InputValue` and **style-token** enums (`TextStyle`, `Tone`, `Spacing`, …).

Because these types never change per app, a single native shell is built **once** and
renders *any* Mobiler app — it only ever knows these types, never an app's domain
events or widgets.

You usually don't depend on this crate directly. Use
[`mobiler-core`](https://crates.io/crates/mobiler-core), which re-exports it and
provides the `MobilerApp` trait plus the typed widget builders.

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
