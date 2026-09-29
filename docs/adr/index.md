# ADR Index

Read this before designing a change. Name the records that apply in the spec (see `README.md`).

| # | Decision | Status | Decided | Superseded by |
|---|---|---|---|---|
| ADR-0001 | The app is a Rust core returning a `Widget` tree over a fixed, facet-generated ABI; the native shells are generic renderers no app edits | Accepted | 2026-05-25 | — |
| ADR-0002 | Every platform capability is an opaque `{plugin, op, input}` effect answered by a `PluginResponse`, so adding one never changes the wire ABI | Accepted | 2026-05-25 | — |
| ADR-0003 | `PluginResponse` stays exactly `{ ok, output: Vec<u8> }`; richer results ride inside `output`, never as new fields | Accepted | 2026-07-19 | — |
| ADR-0004 | Payloads inside `output` are encoded only through crux's `BincodeFfiFormat`; no library crate depends on `bincode` directly | Accepted | 2026-07-19 | — |
| ADR-0005 | An update's `Render` is emitted before its requests, notifications and streams; shells never block the UI on a plugin request | Accepted | 2026-09-21 | — |
| ADR-0006 | Continuous native events reach the core through one keyed streaming primitive (`cx.subscribe` / `cx.unsubscribe`), shipped with its full lifecycle | Accepted | 2026-06-05 | — |
| ADR-0007 | An existing core builder keeps its signature and behaviour — a plugin call's payload stays byte-identical, a widget builder leaves new fields at no-op defaults; new behaviour arrives as a new builder or `with_*` modifier | Accepted | 2026-06-04 | — |
| ADR-0008 | Before 1.0, ABI types grow by appending fields and variants — a known break for full literals and exhaustive matches, marked BREAKING, minor bump | Accepted | 2026-09-27 | — |
| ADR-0009 | A release publishes the libraries first; template shell code using new ABI lands in the CLI PR only after they are live on crates.io | Accepted | 2026-05-31 | — |
| ADR-0010 | One app's product or compliance rule never becomes a framework limitation; the capability ships as an opt-in plugin | Accepted | 2026-06-07 | — |
