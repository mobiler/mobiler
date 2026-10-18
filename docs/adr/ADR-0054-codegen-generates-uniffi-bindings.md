# ADR-0054: An app's `codegen` binary generates its Kotlin and Swift bindings through UniFFI's library mode, not through crux, with UniFFI pinned to exactly the `uniffi_bindgen` version crux_core pins; a move to BoltFFI is a separate decision that supersedes this one

Status:        Accepted
Date decided:  2026-10-18
Deciding PRs:  #298, #299
Supersedes:    none
Code anchor:   mobiler/templates/shared/src/bin/codegen.rs (generate_bindings, built_library), mobiler/templates/shared/Cargo.toml.tmpl (codegen feature, uniffi/cargo_metadata/camino), demos/*/shared/src/bin/codegen.rs
Conformance:   xtask/tests/adr_conformance.rs::adr_0054_codegen_uses_uniffi_not_crux_bindgen

## 1. Context (The Problem)

The template's `shared/src/bin/codegen.rs` made the UniFFI bindings (the `shared.kt` / `shared.swift`
FFI glue) by calling `crux_core::cli::{BindgenArgsBuilder, bindgen}` behind the `crux_core/cli`
feature. crux 0.19 removed that module. crux 0.20's replacement, `crux_core::bindgen::bindgen_kotlin`,
is deprecated and makes Kotlin only, so the iOS shell would get nothing. crux's changelog also
signals a move from UniFFI to BoltFFI in a later release. The 0.20 upgrade
(`docs/superpowers/specs/2026-10-18-crux-0-20-upgrade-design.md`) had to find another way to make
both languages' bindings without changing what the shells see. `[recorded: spec §2]`

## 2. Hypothesis

If the app's own codegen calls `uniffi::generate_bindings_library_mode` with the Kotlin or Swift
generator, using the library that `cargo build` left in the target dir and the cargo-metadata config
supplier (so `shared/uniffi.toml` still applies), then the bindings are byte-identical to what crux's
bindgen made. And as long as UniFFI is pinned to the version crux pins, the scaffolding crux's
macros emit and the bindings the codegen writes agree.

### 2.1. Refutation Conditions

- **Condition 1 — no crux bindgen.**
  Neither the template codegen nor any demo's codegen uses `crux_core::cli` or `crux_core::bindgen`,
  and no shared manifest enables `crux_core/cli`.
  - **Validation Metric:** `xtask/tests/adr_conformance.rs::adr_0054_codegen_uses_uniffi_not_crux_bindgen`.
- **Condition 2 — UniFFI matches crux.**
  The template's and every demo's `uniffi = { version = "=X" }` equals the `uniffi_bindgen`
  requirement crux_core declares (read from `cargo metadata`).
  - **Validation Metric:** the same test.
- **Condition 3 — same output.**
  Old and new codegen produce identical binding files. This was checked once on all five demos for
  both languages during #298: the bindings were byte-identical, and the only type changes came from
  facet_generate 0.19, which was accepted in the spec's Amendment. There is no standing test, because
  the old path no longer compiles against crux 0.20.

## 3. Considered Options & Rationale for Refutation

- **Option A — crux 0.20's `bindgen_kotlin`** `[recorded: spec §2]`
  Deprecated and Kotlin-only. iOS would need a second mechanism anyway, and the deprecation means
  it disappears at the next crux release.
- **Option B — move to BoltFFI now, alongside crux** `[recorded: maintainer discussion 2026-10-18]`
  crux hasn't shipped a BoltFFI release on crates.io. Switching would change the generated FFI layer
  under every shell (both Core files, the iOS module map, the Android JNI loading) and every app in
  the field, during the window when appointments is preparing to go live. It is worth doing as its
  own project, with its own ADR superseding this one, once crux ships it.
- **Option C — keep crux 0.18** `[recorded: spec Why]`
  That leaves mobiler two releases behind, where fixes land too: 0.20 stops reusing effect ids and
  fixes the registry slot leak on fire-and-forget renders.
- **Option D — library mode without cargo metadata** `[recorded: spec §2, builder check]`
  Without the config supplier, `shared/uniffi.toml` (the Kotlin package, `cdylib_name`) is ignored
  and the Kotlin bindings change. So `cargo_metadata` 0.19 (to match `uniffi_bindgen` 0.29.4's
  `From<Metadata>`) and `camino` 1 become optional codegen dependencies.

## 4. Decision & Rationale for Corroboration

The codegen owns its bindings step: `generate_bindings(language, out_dir)` runs cargo metadata, finds
`<target>/debug/lib<name>.{rlib,dylib,a}` (so a custom `CARGO_TARGET_DIR` works), and calls UniFFI's
library mode with the matching generator. The `codegen` feature is `uniffi/bindgen`,
`dep:cargo_metadata`, `dep:camino` in place of `crux_core/cli`. UniFFI stays at `=0.29.4`, which is
crux 0.20's pin. Barbershop ran on the emulator after the switch (#298), and all three iOS CI lanes
compiled the regenerated Swift.

**Mutation proof:** each mutation was reverted afterwards, and each failed
`adr_0054_codegen_uses_uniffi_not_crux_bindgen` with the message shown:
- (a) adding `use crux_core::bindgen::bindgen_kotlin;` to the template codegen gave "ADR-0054:
  …/mobiler/templates/shared uses crux's bindgen (crux_core::bindgen)";
- (b) setting the template's `uniffi` to `=0.29.3` gave "ADR-0054: …/mobiler/templates/shared pins
  uniffi =0.29.3, crux_core pins uniffi_bindgen =0.29.4".

## 5. Consequences (Positive and Negative Predictions)

- Positive: the bindings no longer depend on a crux module that may be removed. Upgrading crux is a
  version bump plus a pin check, not a codegen rewrite.
- Positive: both languages go through one helper, and its output was proven identical to before.
- Negative: an app's codegen now carries about 50 lines of FFI plumbing that crux used to own, and
  the helper exists as six copies (the template and five demos) that must be kept in step by hand.
- Negative: two more optional dependencies in every app's `shared/Cargo.toml`, and `cargo_metadata`
  must track the major version `uniffi_bindgen` uses.
- Negative: when crux moves to BoltFFI, this record is superseded and every app's codegen and shells
  change again.
