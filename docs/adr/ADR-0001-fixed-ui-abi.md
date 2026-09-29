# ADR-0001: The app is a Rust core that returns a `Widget` tree over a fixed, facet-generated ABI; the native shells are generic renderers that no app edits

Status:        Accepted
Date decided:  2026-05-25
Deciding PRs:  none (pre-PR history — commit 7514465 "Prototype: generic mobiler-ui ABI"; e72357b later moved the crates to the root)
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Widget, Action, Theme …), mobiler/templates/{Android,iOS}, demos/*/Android + iOS shells
Conformance:   mobiler-ui/src/lib.rs::shapes_round_trip, mobiler-ui/src/lib.rs::steps_round_trip, mobiler-ui/src/lib.rs::extended_fab_round_trips

## 1. Context (The Problem)

mobiler's promise is one Rust codebase for Android, iOS and the web, with an app author who never
writes Kotlin or Swift. That needs one boundary that every platform understands identically: what
to draw, and what the user did. Crux already provides the effect/event plumbing between a Rust core
and a shell, but not a UI vocabulary, and hand-written per-platform types drift.

## 2. Hypothesis

If the whole UI is described by one Rust enum (`Widget`, with `Action` flowing back) defined once in
`mobiler-ui`, its Kotlin and Swift twins are *generated* from it (facet typegen), and each shell is a
generic `Widget → native view` renderer shared by every app, then:

- an app is written entirely in Rust;
- all three platforms render the same tree;
- a new UI capability is added in one place (the ABI plus each shell's arm) for every app at once.

### 2.1. Refutation Conditions

- **Condition 1 — the ABI types serialize losslessly.** The ABI types must round-trip through
  serde, or a shell would read something different from what the core wrote.
  - **Validation Metric:** the serde round-trip tests in `mobiler-ui/src/lib.rs` (`round_trips(…)`,
    e.g. the three cited above). They go through JSON, so they check lossless serde, not the bincode
    wire; the wire itself is exercised by the shell builds in Condition 2.
- **Condition 2 — shells compile against the generated types.** A generated Kotlin/Swift type that
  a shell can't consume breaks every app.
  - **Validation Metric:** the CI `Android build (demos/*)`, `iOS build (…)` and
    `scaffold + build (template, Android)` lanes.

## 3. Considered Options & Rationale for Refutation

- **Option A — hand-written platform UI per app, with a shared Rust business core** `[reconstructed]`
  The common Crux shape. Rejected because it gives up the one-codebase promise: every app would need
  Kotlin and Swift authors.
- **Option B — a web view everywhere** `[reconstructed]`
  One renderer, but no native widgets, feel or platform capabilities (maps, video, pickers).
- **Option C — a fixed widget vocabulary with generated platform types** `[recorded: commit 7514465 ("defines the fixed wire ABI … The shell imports ONLY the ABI types"); README "The fixed UI wire ABI"]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. The `Widget` enum lives in `mobiler-ui`, and typegen emits the Kotlin and Swift types.
Each shell's `Render` maps every variant. The template shells are copied into each new app by the
CLI and upgraded with `mobiler upgrade`, never edited by the app author. Every release since has
grown the vocabulary this way, from charts and maps to the design-release widgets, without an app
writing platform code.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** one Rust app runs on three platforms, and a widget added once reaches every app.
- **Positive:** the generated types make shell/core skew a compile error, not a runtime surprise.
- **Negative:** every new widget or field touches the ABI crate, all three shells, and every demo's
  exhaustive matches. Adding a variant is a breaking change for any hand-written shell (see
  ADR-0008).
- **Negative:** apps can't draw something the vocabulary lacks. They wait for the framework or file
  a request, which is how the appointments team's asks arrived.
- **Negative:** Swift enum patterns are positional, so appending a field to a widget touches every
  iOS shell's `case` patterns.
