# ADR-0019: Visual design tokens (colour roles, type scale, component shapes) live on `Theme` as optional role tables, and every unset role renders exactly as before

Status:        Accepted
Date decided:  2026-09-27
Deciding PRs:  #221, #228, #230
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Theme, Palette, ColorRoles, TonePair, Rgba, TypeScale, TypeSpec, FamilyRole, Shapes, Radius), mobiler-web/src/lib.rs (theme_css, palette_css), mobiler-web/src/mobiler.css (var(--x, <old value>) fallbacks), template shells (Android ui/theme/Theme.kt paletteScheme + LocalPalette; iOS Render.swift ActivePalette + role(_:else:))
Conformance:   mobiler-web/src/lib.rs::theme_css_without_palette_is_unchanged, mobiler-web/src/lib.rs::theme_css_without_scale_is_unchanged, mobiler-web/src/lib.rs::theme_css_without_shapes_is_unchanged, mobiler-ui/src/lib.rs::theme_default_has_no_palette_and_palette_round_trips, mobiler-ui/src/lib.rs::shapes_round_trip, mobiler-ui/src/lib.rs::type_scale_and_new_styles_round_trip

## 1. Context (The Problem)

Before the Moj Termin design release, `Theme` carried two colours (`seed`, `accent`) plus global
`corner`, `density` and `font` steps. Every other colour, text size and corner radius was baked into
each shell, and each shell baked in different values: Android started from a Material 3 scheme, iOS
used system colours, and the web had a fixed variable set. The design needed 20 colour roles in
light and dark, a per-style type scale and a radius per component. None of that could be expressed.

Apps already existed on the old look (the demos, the appointments app). Whatever carried the new
tokens had to leave those apps looking the same.

## 2. Hypothesis

If every design token is an optional field in an optional table on `Theme` (`palette`,
`type_scale`, `shapes`), and every shell reads each role with its previous value as the fallback,
then:

- an app that sets nothing renders exactly as before, on every shell;
- an app can set one role and leave the rest alone;
- the whole look of an app is set in one place, `Theme`.

### 2.1. Refutation Conditions

- **Condition 1 — no tables set means the old output.** The web theme CSS for `Theme::default()`
  equals the pre-palette string exactly.
  - **Validation Metric:** `theme_css_without_palette_is_unchanged` in `mobiler-web/src/lib.rs`
    compares against a hard-coded string.
- **Condition 2 — an empty table is the same as no table.** A `TypeScale` or `Shapes` with every
  entry `None` gives byte-identical CSS to no table at all.
  - **Validation Metric:** `theme_css_without_scale_is_unchanged` and
    `theme_css_without_shapes_is_unchanged` in `mobiler-web/src/lib.rs`.
- **Condition 3 — the default `Theme` sets no table.**
  - **Validation Metric:** in `mobiler-ui/src/lib.rs`, `theme_default_has_no_palette_and_palette_round_trips`
    (asserts `Theme::default().palette == None` and an unset role is `None`), `shapes_round_trip`
    (`shapes == None`) and `type_scale_and_new_styles_round_trip` (`type_scale == None`).
- **Condition 4 — the native shells fall back per role.** Android and iOS read each role with a
  fallback expression. No unit test covers this; it is checked by review. PR #221 also records an
  Android coffee screenshot that differs from the baseline only in one status-bar icon.

## 3. Considered Options & Rationale for Refutation

- **Option A — an additive `with_palette` scaffold builder instead of a `Theme` field** `[recorded: docs/superpowers/specs/2026-09-27-theme-palette-design.md, decision 1]`
  It would have kept `Theme` literals compiling. The spec records why the break was accepted
  instead: "We accept it, because there is no live app yet (user, 2026-09-27). The type-scale and
  component-shapes requests will extend `Theme` the same way, so the whole look stays in one
  struct."
- **Option B — required tokens, with the core filling in defaults** `[reconstructed]`
  Every shell had different built-in values, so the core could not state "the old look" in one set
  of numbers. Apps would also have had to supply or accept every role.
- **Option C — fix the old look's inconsistencies at the same time** `[recorded: docs/superpowers/specs/2026-09-27-theme-palette-design.md, decision 4]`
  The spec leaves the `None` path's known problems (web dark badges, the avatar dot, undefined web
  variables) alone "because that would change existing looks".
- **Option D — optional role tables on `Theme`, each role falling back to the shell's current value** `[recorded: docs/superpowers/specs/2026-09-27-theme-palette-design.md, decisions 1–2 ("Unset means unchanged."; "The type-scale and component-shapes requests will extend `Theme` the same way, so the whole look stays in one struct."); docs/superpowers/specs/2026-09-28-type-scale-design.md, decision 2; docs/superpowers/specs/2026-09-28-component-shapes-design.md, decision 1]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D. `Theme` gained `palette: Option<Palette>` (PR #221), `type_scale: Option<TypeScale>`
(PR #228) and `shapes: Option<Shapes>` (PR #230). Inside each table every entry is an `Option`.
`None`, at either level, keeps the shell's previous value for that role.

Each shell applies the fallback its own way:

- **Web:** `theme_css` emits a CSS variable only for a set role (`--surface-bar`, `--ts-title-size`,
  `--r-card`, …). `mobiler.css` reads each one with the old value as the fallback, for example
  `var(--surface-bar, var(--bg))`, `var(--ts-title-size, 1.75rem)`, `var(--r-card, var(--radius))`.
- **Android:** `paletteScheme` overrides only the Material slots a set role maps to; widgets read
  `LocalPalette` and keep their default when a role is unset.
- **iOS:** `role(_:else:)` returns the palette colour or the old expression.

Held so far: PRs #221, #228 and #230 each report the coffee and todo web renders pixel-identical
to the pre-change build, and the web tests above pin the no-table output.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an existing app upgrades without any visual change, and a new app can adopt one
  token at a time.
- **Positive:** future tokens have a known shape: another optional table on `Theme`, read with
  fallbacks.
- **Negative:** each new `Theme` field breaks code that writes `Theme { … }` as a full literal. Apps
  must write `..Default::default()` (the note is in `Theme`'s doc comment and the Upgrading section of
  `mobiler-core/README.md`). See ADR-0008.
- **Negative:** the old look's inconsistencies stay on the `None` path on purpose, so the unthemed
  look still differs between shells.
- **Negative:** every role needs a fallback at every place a shell uses it, in three shells. Only the
  web output is pinned by unit tests; Android and iOS rely on review and screenshots. A missed
  dependency on the active role set is easy to write: PR #243 fixed iOS text that kept the dark
  palette's colour after a switch to Light.
