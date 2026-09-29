# ADR-0008: Before 1.0, ABI types grow by appending fields and variants: a known break for full struct literals and exhaustive matches, marked as BREAKING and signalled by a minor version bump

Status:        Accepted
Date decided:  2026-09-27
Deciding PRs:  #221
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Theme, Scaffold, Fab, Badge, Avatar, Grid, CardStyle, Icon, TextStyle …), mobiler-core/README.md "Upgrading"
Conformance:   mobiler-ui/src/lib.rs::shapes_round_trip, mobiler-ui/src/lib.rs::extended_fab_round_trips, mobiler-ui/src/lib.rs::badge_icon_and_avatar_initials_round_trip, mobiler-ui/src/lib.rs::grid_columns_and_dashed_card_round_trip, mobiler-ui/src/lib.rs::steps_round_trip

## 1. Context (The Problem)

The design release needed new fields on existing types: `Theme.palette`, `type_scale` and `shapes`,
`Scaffold.appearance` and `bottom_bar`, and `Fab.label`, among others. Rust has no non-breaking way
to add a public field to a struct that apps can build with a literal, or a variant to an enum that
apps match exhaustively. The alternatives are `#[non_exhaustive]`, which forbids the literals apps
already write, or wrapper/extension types. No production app was live yet.

Fields had been appended this way, with a minor bump, since at least PR #102 (`TextField` gained
`kind` and `error`, ui 0.13 → 0.14). What this record adds is the announcement discipline: accept
the break explicitly, mark it `BREAKING`, and document it in the README's Upgrading section.

## 2. Hypothesis

If new fields and variants are **appended** (never inserted or reordered, so the wire order of the
old ones holds), every such change is marked `BREAKING` (in its doc comment or the crate README's
Upgrading section), and the release takes a **minor** bump (0.x semantics), then:

- apps that use builders and `..Default::default()` are unaffected;
- apps that write full literals or exhaustive matches get a clear compile error that the upgrade
  notes explain;
- the ABI stays readable, with no wrapper types.

### 2.1. Refutation Conditions

- **Condition 1 — appended types round-trip.** Types carrying the new fields must serialize
  losslessly through serde.
  - **Validation Metric:** the `mobiler-ui` serde round-trip tests listed above. They go through JSON,
    so they check lossless serde, not the bincode wire order. Appending (never inserting) is checked
    in review.
- **Condition 2 — breaks are announced.** Review checks that every appended field or variant on an
  app-facing type is marked, with a `BREAKING` note in its doc comment or in the README's Upgrading
  section.

## 3. Considered Options & Rationale for Refutation

- **Option A — `#[non_exhaustive]` on ABI structs and enums** `[reconstructed]`
  Blocks the struct literals apps already write, and forces builder-only construction immediately.
- **Option B — wrapper or "extension" types for new data** `[reconstructed]`
  Keeps old literals compiling, but fragments the ABI, and every shell must merge two sources.
- **Option C — append and announce, pre-1.0** `[recorded: docs/superpowers/specs/2026-09-27-theme-palette-design.md, Decision 1 — "We accept it, because there is no live app yet (user, 2026-09-27)"; PR #221]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C, applied across the whole Moj Termin design release (ui 0.29 / core 0.40). In the
post-release smoke test (2026-09-29; not recorded elsewhere in the repo), a 0.57.1 app upgraded to
0.58.0 with `mobiler upgrade --apply` merged its shells with zero conflicts and built without manual
steps.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the ABI stays one plain set of types, and apps on builders don't notice growth.
- **Negative:** every appended field or variant touches every shell's exhaustive code: Swift
  positional patterns and Kotlin `when`s, in every demo and the template.
- **Negative:** this is a pre-1.0 stance. At 1.0 it must be superseded by a real compatibility policy
  (for example `#[non_exhaustive]` plus builders only).
