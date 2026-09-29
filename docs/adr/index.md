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
| ADR-0011 | `mobiler plugin add` installs a plugin by copying its native sources into the app and inserting manifest-declared lines above `mobiler:*` anchor comments; `mobiler upgrade` preserves those lines but never rewrites plugin bodies, and reports a drifted plugin instead | Accepted | 2026-05-30 | — |
| ADR-0012 | `mobiler upgrade` refreshes an app's generated files by a three-way merge against the stored `.mobiler/base/` snapshot; a conflict is left as `<file>.mobiler-new` and never applied, and the app's own code is never touched | Accepted | 2026-06-01 | — |
| ADR-0013 | A request's `ok` reports what the platform actually answered: a hand-off the platform refuses or can't route is `ok: false` with a reason — never a crash, and never an unconditional `ok: true` | Accepted | 2026-06-02 | — |
| ADR-0014 | Large touch targets are one scaffold-wide `Density::Large` mode, not per-widget size fields | Accepted | 2026-09-17 | — |
| ADR-0015 | Barbershop's native shells are the reference: a release builds its shell changes in `demos/barbershop` first, and the CLI templates are ported from barbershop's diff in the release's CLI PR | Accepted | 2026-09-17 | — |
| ADR-0016 | Every string a shell draws itself comes from the app — a per-call label, else the root scaffold's `ShellLabels`, else a built-in English default; shells ship no translations of their own | Accepted | 2026-09-22 | — |
| ADR-0017 | At most one shell-owned confirm and one snackbar are open at a time; a new one answers the open one `ok: false` and replaces it, so every call's continuation resolves exactly once | Accepted | 2026-09-22 | — |
| ADR-0018 | The app's version, build, platform and bundle id reach the core as an `Action::AppInfo` that every shell sends before `Restore` / `Start`; the core keeps it process-wide so `cx.app_info()` is already set in `init` | Accepted | 2026-09-27 | — |
| ADR-0019 | Visual design tokens (colour roles, type scale, component shapes) live on `Theme` as optional role tables, and every unset role renders exactly as before | Accepted | 2026-09-27 | — |
| ADR-0020 | The app chooses Light, Dark or System on the scaffold; under System the shell follows the OS live without a core update, and the OS value is readable by query and stream | Accepted | 2026-09-28 | — |
| ADR-0021 | The iOS deployment target is 17.0 in the template and every demo with an iOS project, so the iOS shell uses iOS 17 APIs without availability checks | Accepted | 2026-09-28 | — |
| ADR-0022 | Custom fonts are files declared in the app-root `mobiler.toml` `[fonts]` and copied into every shell by `mobiler fonts sync`; the ABI carries only a family reference, never font bytes | Accepted | 2026-09-28 | — |
