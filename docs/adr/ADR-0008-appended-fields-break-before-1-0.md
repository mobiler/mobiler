# ADR-0008: Before 1.0, ABI types grow by appending fields and variants: a known break for full struct literals and exhaustive matches, marked as BREAKING and signalled by a minor version bump

Status:        Accepted
Date decided:  2026-09-27
Deciding PRs:  #221
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Theme, Scaffold, Fab, Badge, Avatar, Grid, CardStyle, Icon, TextStyle …), mobiler-core/README.md "Upgrading"
Conformance:   mobiler-ui/src/lib.rs round-trip tests (shapes_round_trip, extended_fab_round_trips, badge_icon_and_avatar_initials_round_trip, grid_columns_and_dashed_card_round_trip, steps_round_trip)

## 1. Context (The Problem)

The design release needed new fields on existing types: `Theme.palette`, `type_scale` and `shapes`,
`Scaffold.appearance` and `bottom_bar`, and `Fab.label`, among others. Rust has no non-breaking way
to add a public field to a struct that apps can build with a literal, or a variant to an enum that
apps match exhaustively. The alternatives are wrapper types or `#[non_exhaustive]`, which also forbid
the literals apps already write. No production app was live yet.

## 2. Hypothesis

If new fields and variants are **appended** (never inserted or reordered, so the wire order of the
old ones holds), every such change is marked `BREAKING` in its doc comment and in the crate README,
and the release takes a **minor** bump (0.x semantics), then:

- apps that use builders and `..Default::default()` are unaffected;
- apps that write full literals or exhaustive matches get a clear compile error that the upgrade
  notes explain;
- the ABI stays readable, with no wrapper types.

### 2.1. Refutation Conditions

- **Condition 1 — appended types round-trip.** Types carrying the new fields must serialize
  losslessly.
  - **Validation Metric:** the `mobiler-ui` round-trip tests listed above.
- **Condition 2 — breaks are announced.** Review checks that every appended field on an
  app-constructible type carries a `BREAKING` note and a README "Upgrading" entry.

## 3. Considered Options & Rationale for Refutation

- **Option A — `#[non_exhaustive]` on ABI structs and enums** `[reconstructed]`
  Blocks the struct literals apps already write, and forces builder-only construction immediately.
- **Option B — wrapper or "extension" types for new data** `[reconstructed]`
  Keeps old literals compiling, but fragments the ABI, and every shell must merge two sources.
- **Option C — append and announce, pre-1.0** `[recorded: maintainer decision during the palette work, "we do not have any live app yet"; PR #221]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C, applied across the whole Moj Termin design release (ui 0.29 / core 0.40). The
0.57.1 → 0.58.0 `mobiler upgrade --apply` test merged the shells with zero conflicts, and the
upgraded app built without manual steps.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the ABI stays one plain set of types, and apps on builders don't notice growth.
- **Negative:** every appended field or variant touches every shell's exhaustive code: Swift
  positional patterns and Kotlin `when`s, in every demo and the template.
- **Negative:** this is a pre-1.0 stance. At 1.0 it must be superseded by a real compatibility policy
  (for example `#[non_exhaustive]` plus builders only).
