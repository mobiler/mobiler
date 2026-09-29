# ADR-0014: Large touch targets are one scaffold-wide `Density::Large` mode, not per-widget size fields

Status:        Accepted
Date decided:  2026-09-17
Deciding PRs:  #205
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Density, Theme.density), mobiler-web/src/lib.rs (the `density-large` scaffold class, theme_css spacing), mobiler-web/src/mobiler.css (`.density-large` rules), template shells (Android MainActivity.kt isLarge; iOS Render.swift isLargeDensity + LargeButtonStyle)
Conformance:   mobiler-ui/src/lib.rs::widget_round_trips

## 1. Context (The Problem)

The appointments team asked for bigger, more forgiving controls for "busy hands": staff using the
app in a hurry, with wet or gloved hands. Buttons, chips, segmented controls, icon buttons and
calendar days all needed larger tap targets, larger control labels and more space between
tappables. `Theme` already had a global `density` (`Compact`, `Comfortable`, since PR #27), set once
on the scaffold.

## 2. Hypothesis

If large controls are a third value of the existing scaffold-wide density, `Density::Large`, and
each shell sizes every control from that one mode, then:

- an app switches its whole UI to large targets with one theme value, and can let the user toggle
  it at runtime;
- the sizes stay consistent across widgets and across shells, because the framework picks them
  (56 buttons and segmented controls, 48 chips and calendar days, 56 icon-button targets, 16
  control labels, at least 12 between adjacent tappables);
- no widget type gains a size field, so the ABI and the builders stay as they were;
- apps on `Compact` or `Comfortable` render exactly as before.

### 2.1. Refutation Conditions

- **Condition 1 — the mode is on the wire as a theme value.** A themed scaffold with
  `density: Density::Large` round-trips.
  - **Validation Metric:** `widget_round_trips` in `mobiler-ui/src/lib.rs` (its themed-scaffold
    case with `Density::Large`).
- **Condition 2 — no per-widget size.** No widget gains a size, height or density field for this
  purpose. Nothing mechanical checks this; review enforces it.
- **Condition 3 — the other densities are unchanged.** With `Compact` or `Comfortable`, every shell
  passes the pre-change values. PR #205 records this as a review rule ("Pixel identity"), not a
  test.

## 3. Considered Options & Rationale for Refutation

- **Option A — per-widget size fields (a size on `Button`, `Chip`, `Segmented`, …)** `[recorded: docs/superpowers/plans/2026-09-17-large-touch-targets.md, "Agreed decisions": "No per-widget sizes in the app API."]`
  Rejected. The plan records the rejection but not the reason. The reasons here are
  `[reconstructed]`: every control type would gain a field (an ABI change per widget, ADR-0008),
  apps would have to set it on every call, and sizes could drift between screens.
- **Option B — scale text too** `[recorded: same plan, "Agreed decisions": `Density::Large` changes "**control sizes, control spacing, and control label sizes only**"]`
  Rejected. Body text is left to the platform's font-scale setting (Android `fontScale`, iOS
  Dynamic Type, the browser font size).
- **Option C — one theme-level mode, `Density::Large`** `[recorded: same plan, "Goal" ("a theme-level `Density::Large`"); PR #205 ("bigger, forgiving controls behind one theme-level switch")]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. `Density` gained `Large` (PR #205, commit 9b6929c; released as mobiler-ui 0.24). Each
shell reads the scaffold's theme once and sizes controls from it: the web adds a `density-large`
class to the scaffold and `mobiler.css` scopes the sizes under it; Android reads `isLarge`; iOS reads
`isLargeDensity()` and uses a `LargeButtonStyle`. The spacing multiplier from the old densities is
kept, with `Large` as the largest step.

Per-widget choices that are not about size still arrived as widget fields in the same PR (button
`tone`, `icon`, `wide`, through `button_with` and `ButtonOpts`), which keeps this decision to size
only. Later work built on the mode rather than around it: `Theme.shapes` keeps pill buttons pill
at `Density::Large` (PR #230).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** one theme value, which an app can flip at runtime (barbershop's "Large controls"
  toggle), changes every control consistently on all shells.
- **Positive:** no widget or builder signature changed for size.
- **Negative:** an app cannot make one button large and another normal. A screen that needs a
  single oversized control has no way to ask for it.
- **Negative:** the sizes are framework constants. An app that wants 52 instead of 56 cannot tune
  them.
- **Negative:** every control a shell draws must check the mode, in three shells. A new control
  added later without a `Large` branch is silently small, and nothing mechanical catches it.
