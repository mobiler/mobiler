# crux 0.20 upgrade

**Asked by:** the maintainer, 2026-10-18, after a dependency check: "we are planning to go live with appointments, but
nothing put in stone... so we should upgrade at least crux". The design was played back in chat and approved
("agree totally"), including keeping UniFFI and moving apps via `mobiler upgrade`.
**Release:** mobiler-ui 0.32 / mobiler-core 0.47 / mobiler-web 0.47, then CLI 0.70.
**Constrained by:**
- ADR-0001: the ABI is facet-generated; the generated `SharedTypes` must not change (§5 proves it).
- ADR-0004: payloads stay on crux's `BincodeFfiFormat`. crux 0.20 still ships it (`crux_core::bridge::BincodeFfiFormat`)
  and still pins `bincode =1.3`.
- ADR-0008: a dependency change that forces apps to move too is a minor bump of every library.
- ADR-0009 / ADR-0030: libraries are published first, by hand, in order (ui → core → web); the template that pins the
  new core lands in the CLI PR after they are live, and the CLI ships by tag.
- ADR-0015: barbershop is migrated first; the other demos and the template follow its diff.
- ADR-0023: the web shell moves by a version bump of `mobiler-web`.
- ADR-0046: `mobiler upgrade`'s merge and review rules are unchanged; §3 only widens what its dependency step edits.

**New records:** ADR-0054 (the app's codegen generates its UniFFI bindings itself, §2) and ADR-0055 (`mobiler upgrade`
moves the framework's dependency pins in an app's manifests, §3).

## Why

crux 0.20 fixes two bridge bugs that every Mobiler app has today:

- **Render leak.** Every fire-and-forget request (each `Render`) took a bridge registry slot that was never freed.
  crux estimates ~1 MB a day for an app rendering ten times a second. A long-open screen (the appointments admin app)
  grows without bound.
- **Response to the wrong request.** Effect ids were reused slab slots. A shell resolving an id twice (retry, race,
  bug) could deliver one request's answer to an unrelated later request, silently when the payloads decode alike
  (two HTTP responses do). In 0.20 ids are never reused, so a second resolve is a "not found" error.

crux states the FFI wire is unchanged and the generated Swift/Kotlin types are byte-for-byte the same. Appointments is
heading for production, so this should land before go-live.

## Goals

- Libraries, template, every demo and the CLI's own workspace on `crux_core 0.20.0` and `facet =0.46.5`.
- An existing app moves with `mobiler upgrade --apply` and builds, with no manual Cargo edits.
- Generated Kotlin and Swift (types and UniFFI bindings) are byte-identical to today's for the same app.

## Non-goals

- **BoltFFI.** crux 0.19 moved its examples to BoltFFI and deprecated its UniFFI support. We stay on UniFFI: our FFI
  surface is three byte-passing functions (`update` / `resolve` / `view`), so BoltFFI's speed gives us nothing, and it
  is eight months old, pre-1.0, with 30 releases so far, while switching would change every app's Android and iOS
  build. It is a deliberate follow-up project (evaluate once BoltFFI is 1.0 or crux removes UniFFI). §2 makes that
  move independent of crux's timetable.
- **uniffi 0.32.** crux 0.20 itself pins `uniffi_bindgen =0.29.4`; we stay on `=0.29.4`.
- leptos 0.8, and major bumps of CLI-only crates (toml, diffy, sha2, gloo-*): not in this release.
- crux 0.21 (in crux's changelog, not on crates.io): not targeted. Note for later: it removes serde typegen, which we
  do not use (we are on `facet_typegen`).
- Changing `ffi.rs`. It panics on a bridge error. Under 0.20 a double resolve is a `NotFound` error and so a panic,
  where before it could be a silent misdelivery. Our shells resolve each request once (ADR-0017), and `ffi.rs` is an
  app-owned file `upgrade` never touches, so it stays as is.

## Anti-goals

- A changed `SharedTypes` or binding file, however small: that would be an ABI change hidden in a dependency bump.
- An upgraded app that compiles two `crux_core` or two `facet_core` copies (the "`Facet<'_>` is not satisfied" error
  crux warns about).
- `mobiler upgrade` rewriting anything in an app's manifests beyond the exact pin lines named in §3.

## Decisions

### 1. Libraries (one PR, then publish)

- Root workspace: `crux_core = "0.20.0"`; `facet = "=0.46.5"` in `mobiler-ui`, `mobiler-core` and `xtask`.
- `mobiler-web`: `crux_core = "0.20"`.
- Versions: ui 0.32.0, core 0.47.0, web 0.47.0; core and web depend on the new ui / core.
- `cargo update` for the root and `mobiler-web` lockfiles rides along (59 semver-compatible bumps, e.g. serde 1.0.229,
  clap 4.6.7, facet_generate 0.17.2 → whatever 0.20 needs).
- Nothing in the libraries uses `crux_core::effects::EffectId` (renamed `ParkedEffectId` in 0.20); the build confirms.

### 2. Bindings: the app's codegen calls UniFFI directly (ADR-0054)

Today `shared/src/bin/codegen.rs` calls `crux_core::cli::{BindgenArgsBuilder, bindgen}` for Kotlin and Swift, behind
the `crux_core/cli` feature. crux 0.19 removed both. crux 0.20's replacement, `crux_core::bindgen::bindgen_kotlin`
(feature `bindgen`), is deprecated and Kotlin-only, so the iOS path would have nothing.

The template's codegen generates both languages with UniFFI's library mode:

- `shared/Cargo.toml` `codegen` feature: drop `"crux_core/cli"`, add `"uniffi/bindgen"`, `"dep:cargo_metadata"` and
  `"dep:camino"`. UniFFI 0.29.4's `bindgen` feature exposes `generate_bindings_library_mode`,
  `CargoMetadataConfigSupplier` and the Kotlin and Swift generators, but not the `cargo_metadata` crate (needed to
  build the config supplier, which is what applies `shared/uniffi.toml`'s Kotlin package and `cdylib_name`) or
  `camino` (`generate_bindings` takes a `Utf8Path`). So `shared/Cargo.toml` gains two optional dependencies:
  `cargo_metadata = { version = "0.19", optional = true }` (must match `uniffi_bindgen` 0.29.4's `^0.19`, or the
  `From<Metadata>` impl does not apply) and `camino = { version = "1", optional = true }`. Skipping cargo metadata
  is not an option: it would drop `uniffi.toml` and change the Kotlin bindings. *(Builder-check finding; settled by
  the designer.)*
- `codegen.rs`: replace the two `bindgen(...)` calls with one helper that finds the crate's built library and calls
  `uniffi::generate_bindings_library_mode` with the Kotlin or Swift generator. crux 0.20's `bindgen_kotlin` is the
  model (cargo metadata → `lib<name>` in `<target>/debug` → library mode); it must resolve the same library the
  scaffold's build uses, including under a custom `CARGO_TARGET_DIR` (cargo metadata reports it).
- The file keeps its `mobiler:codegen-types` anchor and its `register_type` list unchanged.
- **Proof of equivalence:** run the old and new codegen on barbershop for both languages and diff the whole output
  directories. They must be identical. Any difference (file names such as `sharedFFI.modulemap`, the Swift module
  name, the Kotlin package) is fixed in the helper until they are, never accepted.

ADR-0054 records: the bindings are produced by the app's own codegen through UniFFI, not through crux; UniFFI stays
pinned to the version crux pins; a move to BoltFFI is its own decision, with its own ADR superseding this one.

### 3. `mobiler upgrade` moves the framework's pins (ADR-0055)

`Cargo.toml` files are app-owned (`Class::Own`), and upgrade's dependency step only rewrites `mobiler-core = "…"` in
`shared/Cargo.toml`. With core 0.47 needing crux 0.20, an app still pinning `crux_core = "0.18.0"` builds two crux
copies and fails. The dependency step is widened to these exact edits, each a single-line replacement made only when
the line is found in its known form (the last two are the only insertions, both anchored to a line the template
wrote):

| File | Line | To |
|---|---|---|
| `shared/Cargo.toml` | `mobiler-core = "…"` | the template's pin (unchanged behaviour) |
| `Cargo.toml` (workspace) | `crux_core = "…"` under `[workspace.dependencies]` | the template's pin |
| `shared/Cargo.toml` | `"crux_core/cli",` inside the `codegen` feature | `"uniffi/bindgen",` `"dep:cargo_metadata",` `"dep:camino",` (one per line, same indent) |
| `shared/Cargo.toml` | after the `uniffi = { version = "…", optional = true }` line in `[dependencies]` | insert the `cargo_metadata` and `camino` lines of §2, if absent |

- The targets are read from the CLI's embedded templates (as `template_core_version()` does), never hard-coded.
- A line not in a recognised form is left alone and named in the report with the edit to make by hand
  (as the existing "couldn't find a `mobiler-core` dependency" note does). `--apply` is still needed to write; a plain
  run reports what it would change.
- Re-running is a no-op (a pin already at the target, the feature already migrated, the two dependencies present).
- **Other crates in the app.** Upgrade also scans the app's other `Cargo.toml` files (workspace members, a `web/`
  crate) for a `crux_core`, `mobiler-core` or `mobiler-web` pin it would not move, and names each in the report as a
  hand edit with the target version. It does not edit them: their layout is the app's own. *(Designer's call.)*
  Appointments, the app this release is for, has exactly the template layout (root `Cargo.toml` with
  `crux_core = "0.18.0"`, `shared/Cargo.toml` with `mobiler-core = "0.46"`, the `codegen` feature and `uniffi`, no
  other crate, no `mobiler-web`; checked 2026-10-18), so the table covers it with no hand edits.
- `codegen.rs` is already Merge-class, so its new body arrives by the normal three-way merge; an app that added its
  own `register_type` lines keeps them.
- The `facet` pin is not edited: apps get facet through `mobiler-core` and `crux_core`, and the template declares no
  facet dependency. If an app declares its own, upgrade warns (it cannot know the right fix).

ADR-0055 records: upgrade may edit an app's manifests only to move or add the dependency lines the framework owns,
each anchored to a line the template wrote and listed in the CLI; anything else in a manifest stays the app's, and a
framework pin it cannot place is reported, never guessed.

### 4. Template and demos

- Template: workspace `crux_core = "0.20.0"`, `shared/Cargo.toml` as §2, `mobiler-core = "0.47"`, new `codegen.rs`.
- Demos, barbershop first (ADR-0015), then coffee, saldo, todo, fullstack-todo/mobile, fullstack-sqlx (its
  `app-core` pins `crux_core = "0.18.0"` and has no codegen). Each demo's `codegen.rs` and manifests take the same
  diff; while the libraries are unpublished the demos use path/patch dependencies the way earlier releases did, and
  move to the published versions in the CLI PR.

### 5. Verification

- `cargo test --workspace` (root) and `mobiler-web`'s tests; every demo's CI lane (Android, iOS, web).
- **Byte-identical output:** §2's diff of the generated Kotlin and Swift (types + bindings) for barbershop, old vs new.
  Record the command and result in the PR body. A difference in the bindings is fixed in the helper. A difference in
  the types files can only come from facet_generate 0.19 (crux requires it, so it cannot be pinned back): then the
  work **stops** and the diff goes to the maintainer before anything is published, whatever its size. *(Designer's
  call.)*
- **Emulator:** barbershop on Android (the separate `mobiler_pixel7` AVD, `--device emulator-5560`): launch, navigate,
  a plugin round-trip and an HTTP request. iOS rests on the CI lanes (byte-identical bindings make a behaviour change
  there unlikely); a TestFlight build is optional.
- **Upgrade path:** with the published CLI 0.69.1, scaffold an app and add a `register_type` line to its `codegen.rs`;
  then upgrade with the new CLI and `mobiler build android`. Expect: crux, core and the feature line moved, the
  custom line kept, the APK builds. A second run is a no-op; a fresh 0.70 scaffold upgrades as a no-op.
- **Conformance tests** (in `xtask/tests/adr_conformance.rs`, each proven by a mutation recorded in the ADR):
  - ADR-0054: the template's `codegen.rs` names no `crux_core::cli` or `crux_core::bindgen`, and the `uniffi` pin in
    the template equals the `uniffi_bindgen` version crux pins (read from `Cargo.lock`).
  - ADR-0055: the template's `crux_core` pin equals the root workspace's, and an upgrade test proves the three edits
    and the no-op re-run.
- Fresh code + security review of each PR, and a re-review of every fix pass.

### 6. Rollout

1. Libraries PR (§1 + barbershop on path deps + ADR-0054/0055 drafts if they cite no template code yet) → publish
   ui → core → web (maintainer confirms each publish).
2. CLI PR: template, upgrade (§3), all demos on published versions, ADRs, conformance tests, CLI 0.70.0 → tag.
3. Post-release: published-CLI scaffold + the upgrade test above; a note to appointments (run `mobiler upgrade --apply`,
   then build; what the two fixes mean for them).

## Left to the builder

- The helper's exact shape in `codegen.rs`, as long as §2's diff is empty.
- How the libraries PR keeps the demos building before publish (path deps or `[patch.crates-io]`), following the last
  release that did it.
- Whether ADR-0054 and ADR-0055 land in the libraries PR or the CLI PR, as long as each lands with the code it
  describes.

## Amendment 2026-10-18: the accepted types diff

§5's stop rule triggered on barbershop. The UniFFI bindings (`shared.kt`, `shared.swift`, `sharedFFI.h`,
`sharedFFI.modulemap`) are byte-identical. The facet-generated types differ, from facet_generate 0.19 (required by
crux 0.20):

- `Types.kt`: one doc comment on `Request` re-wrapped. No code change.
- `SharedTypes.swift`: every type is declared `Hashable, Equatable` instead of `Hashable`; `Widget` and `Sheet`, which
  had no conformance, are now `Hashable, Equatable`. The serialize/deserialize code is identical, so the wire is
  unchanged. No shell declares its own conformance for `Widget` or `Sheet`.

The maintainer accepted this diff ("yes"). The gate is the three iOS CI lanes compiling every demo against the new
types; a failure there stops the release before publish. Every other demo's diff must be this same pattern and
nothing else.
