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

The `Widget` vocabulary covers layout (rows/columns/grids/cards/scrollers, paged
**lazy lists**), inputs (text fields, toggles, segmented, search, rating), navigation
(scaffold + tabs + FAB + sheets, and a two-pane **master-detail** split), media (images,
avatars, a controllable **video player**, an embedded **web view**, an in-app **PDF viewer**,
and an interactive **map**), gestures (tappable cards with **tap + long-press**, swipe actions),
feedback (progress, skeleton), an inline calendar,
**data-viz charts** — `Chart` in eight styles (bar, line, stacked, 100%-stacked, pie, donut,
fitness-style progress rings, and a radial gauge) plus a variable-width stacked-region /
coverage-gap `RegionChart` — and an **accessibility** wrapper (a screen-reader label / hint /
role on any widget).

You usually don't depend on this crate directly. Use
[`mobiler-core`](https://crates.io/crates/mobiler-core), which re-exports it and
provides the `MobilerApp` trait plus the typed widget builders.

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
