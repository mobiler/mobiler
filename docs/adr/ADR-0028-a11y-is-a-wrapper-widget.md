# ADR-0028: Screen-reader metadata is one `Widget::A11y` wrapper around any subtree, not label, hint or role fields on each widget

Status:        Accepted
Date decided:  2026-06-09
Deciding PRs:  #142, #143
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Widget::A11y, A11yRole), mobiler-core/src/lib.rs (a11y, with_a11y_hint, with_a11y_role), mobiler-web/src/lib.rs (the Widget::A11y arm, a11y_role_aria), template shells (Android MainActivity.kt `is Widget.A11y`; iOS Render.swift `.a11y` arm + a11yTraits)
Conformance:   none — no test builds or round-trips a `Widget::A11y` (it is missing from `widget_round_trips`), and the shell arms are Kotlin, Swift and DOM code with no unit-test harness; review enforces that no widget gains its own label/role field

## 1. Context (The Problem)

Some widgets have nothing a screen reader can announce. An `IconButton { icon, on_press }` and an
`Image { source, shape, ratio }` carry no text of their own (Android announces an icon button by
its icon's enum name). A `Card` of several texts is read out piece by
piece. Batch 6 of the roadmap had to let an app name such elements, say what activating them does,
and say what kind of control they are, on all three shells.

There were two shapes available. Each affected widget could get its own `label` / `hint` / `role`
fields, or one new variant could wrap any subtree. Every new widget field is an ABI change
(ADR-0008) that touches all three shells and every exhaustive match (ADR-0001).

## 2. Hypothesis

If accessibility metadata is one wrapper variant,
`Widget::A11y { child: Box<Widget>, label: String, hint: Option<String>, role: Option<A11yRole> }`,
which presents its whole subtree as one screen-reader element on Android and iOS, then:

- any widget, present or future, can be named without an ABI change to that widget;
- each shell carries exactly one render arm for accessibility (render the child, apply the
  platform's modifier);
- apps that don't use it render and announce exactly as before.

### 2.1. Refutation Conditions

- **Condition 1 — no per-widget accessibility fields.** No widget variant gains a label, hint or
  role field whose purpose is screen-reader metadata.
  - **Validation Metric:** review. Nothing mechanical checks it.
- **Condition 2 — the wrapper round-trips and each shell renders it.** A `Widget::A11y` crosses the
  wire unchanged and every shell has an arm for it.
  - **Validation Metric:** review, plus the shell compile lanes (an exhaustive `match` / `when` /
    `switch` without the arm fails to compile). No unit test covers it today.

## 3. Considered Options & Rationale for Refutation

- **Option A — label / hint / role fields on each widget (`IconButton`, `Image`, `Card`, …)** `[reconstructed]`
  Not chosen. Nobody wrote the rejection down. The costs are inferred: one ABI change per widget
  (ADR-0008), a field every new widget would have to remember, and no way to group several
  children into one announced element.
- **Option B — one wrapper variant over any subtree** `[recorded: commit 8f637ab ("an opt-in accessibility wrapper"); mobiler-ui/src/lib.rs, the `Widget::A11y` doc comment: "presents `child`'s subtree as ONE screen-reader element named by `label`"]`
  Chosen. It follows the existing `Box<Widget>` wrapper shape (`Card`, `Split`).
- **Option C — derive names automatically from the widget's content** `[reconstructed]`
  Not chosen. An icon or an image has no text to derive from, which is the case that needed
  solving.

## 4. Decision & Rationale for Corroboration

Option B. PR #142 added `Widget::A11y` and `A11yRole { Button, Link, Image, Header, Adjustable }` in
mobiler-ui 0.21, the `a11y(child, label)` builder with `with_a11y_hint` / `with_a11y_role` in
mobiler-core 0.28, and one arm per shell: the web renders
`<div class="a11y" role=… aria-label=… title=hint>`; iOS applies
`.accessibilityElement(children: .combine)` with label, hint and `a11yTraits(role)`; Android wraps
the child in `Box(Modifier.semantics(mergeDescendants = true) { contentDescription; role; heading() })`.
PR #143 added the arms to the CLI templates after the libraries published (ADR-0009).

The modifiers set the field on an existing wrapper or wrap a bare widget, so they compose in any
order. Since then no widget has gained its own screen-reader field. The extended FAB's `label` is
drawn on screen and also serves as its name; that is visible text, not a parallel accessibility
field.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a widget added later can be named with no change to it; one arm per shell covers
  accessibility for the whole vocabulary.
- **Positive:** the wrapper also groups: a card's children can be announced as one element.
- **Negative:** on Android and iOS the wrapper always merges its subtree into one element
  (`.combine` / `mergeDescendants`). An app cannot name one control inside a group without
  flattening the group.
- **Negative:** the web does not merge. Its `<div role=… aria-label=…>` (default `role="group"`)
  labels the group, but screen readers still reach each child. The three shells announce the same
  wrapper differently.
- **Negative:** Android has no separate hint. It joins them into one `contentDescription`
  (`"label. hint"`).
- **Negative:** roles map best-effort. Android's Compose `Role` has no link or adjustable, so
  `Link` and `Adjustable` set no role there; SwiftUI has no adjustable trait, so `Adjustable` adds
  nothing on iOS.
- **Negative:** `with_a11y_hint` or `with_a11y_role` on a bare widget wraps it with an empty
  `label`. An app that forgets `a11y(…)` gets an element with no name, and nothing warns.
- **Negative:** accessibility is opt-in per call site. An unlabeled icon button stays unlabeled
  unless the app author wraps it.
