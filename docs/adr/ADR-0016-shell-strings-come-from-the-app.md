# ADR-0016: Every string a shell draws itself comes from the app — a per-call label, else the root scaffold's `ShellLabels`, else a built-in English default; shells ship no translations of their own

Status:        Accepted
Date decided:  2026-09-22
Deciding PRs:  #210 (per-call confirm/picker labels, `end_label`), #213 (`ShellLabels` on the scaffold, the precedence); extended by #215 (`pdf_error`, `pdf_title`, `web_title`) and #237 (`step_of`)
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (ShellLabels, Widget::Scaffold.labels), mobiler-core/src/lib.rs (with_labels, step_text), mobiler-core/src/dialog.rs (Confirm, Picker), mobiler-web/src/lib.rs (ACTIVE_LABELS, shell_label), template shells (iOS Render.swift `ActiveLabels`, Core.swift DialogPlugin / DateTimePlugin; Android MainActivity.kt `ActiveLabels`, Core.kt DialogPlugin / DateTimePlugin)
Conformance:   mobiler-core/src/lib.rs::with_labels_sets_scaffold_labels_and_combinators_keep_them, mobiler-core/src/lib.rs::steps_builder_clamps_and_step_text_fills_the_template

## 1. Context (The Problem)

Some text is drawn by the shells, not by the app's widget tree: confirm and picker buttons, the
`Split` back button, the scaffold back button's accessible name, the web list's "Load more" and
"↻ Refresh", the Android PDF error, web iframe titles, and the step indicator's spoken text. Until
PR #210 (2026-09-22), the ones that existed were hard-coded English in each shell (for example
"End of list" at the end of a `LazyList`, and `window.confirm` on web).

The appointments app is in Serbian and does not edit generated shell code. The spec of 2026-09-21
states the constraint: "the app will not patch generated shell code"
(`docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md`). Shell text therefore
had to become something the app sets through the ABI.

Some of that text belongs to no call at all. The back button and the list controls are drawn during
render, so a per-call label cannot reach them.

## 2. Hypothesis

If every shell-drawn string is resolved as (1) a label passed to that call (`confirm_with`,
`Picker`), then (2) the root scaffold's `ShellLabels`, then (3) the shell's built-in English
default, with an empty string treated as absent, then:

- an app can show every piece of shell text in its own language without touching shell code;
- an app that sets nothing looks exactly as before;
- the shells never need a translation table, so adding a language never needs a framework release.

### 2.1. Refutation Conditions

- **Condition 1 — the app's labels survive the scaffold combinators.** Labels set with
  `with_labels` must still be on the scaffold after `with_theme`, `with_refresh`, `with_fab` and
  `with_sheet`. Otherwise the shell silently falls back to English.
  - **Validation Metric:** `with_labels_sets_scaffold_labels_and_combinators_keep_them` in
    `mobiler-core/src/lib.rs`.
- **Condition 2 — a templated label fills in, and falls back when empty.** The web shell's
  `step_text` fills `ShellLabels.step_of` and falls back to "Step {current} of {total}" for `None`
  or `""`. Android and iOS have their own copies of this function, which the test does not cover.
  - **Validation Metric:** `steps_builder_clamps_and_step_text_fills_the_template` in
    `mobiler-core/src/lib.rs`.
- **Condition 3 — the precedence is the same in every shell.** Per-call, then scaffold, then
  English, with empty treated as absent. Not tested: it lives in Kotlin, Swift and web DOM code.
  Checked by review, and by the runtime checks recorded in PR #213 (web CDP and Android emulator).
- **Condition 4 — no new shell-drawn English string.** Review. A string audit is recorded in PR #213
  ("A string audit found no unguarded English shell literal"). A grep on 2026-09-29 found one gap,
  listed in §5.

## 3. Considered Options & Rationale for Refutation

- **Option A — shells ship their own translations (platform string resources, `NSLocalizedString`, a web table)** `[reconstructed]`
  The shells would need a table for every language an app might use, and a new language would mean
  a framework release. The app's language choice (it may differ from the device locale) would not
  reach the shell's text. Nobody recorded this option; it is the obvious alternative.
- **Option B — per-call labels only** `[recorded: PR #210 and docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md]`
  Shipped first (#210: `confirm_with`, `pick_date_with` / `pick_time_with`, `with_end_label`). It
  can't reach text drawn during render. The follow-up spec lists it:
  "Some text is drawn by the shells themselves, and the app can't change it:"
  (`docs/superpowers/specs/2026-09-22-shell-labels-catalog-design.md`). PR #210 also logs:
  "The web "Load more" / "↻ Refresh" controls are still English."
  Kept as the top of the precedence, not replaced.
- **Option C — per-call labels, then app-wide `ShellLabels` on the scaffold, then English** `[recorded: PR #213 ("**Precedence:** a per-call label (`confirm_with` / `Picker`) wins, then the scaffold label, then today's English default. Empty labels fall back too."); docs/superpowers/specs/2026-09-22-shell-labels-catalog-design.md, "Precedence, for each piece of text"]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. `ShellLabels` is a wire type in `mobiler-ui` with one optional field per shell-drawn
string (`back`, `load_more`, `refresh`, `ok`, `cancel`, `done`, `pdf_error`, `pdf_title`,
`web_title`, `step_of`). It lives on `Widget::Scaffold.labels` and is set once with `with_labels`.
PR #213: "Each shell reads the labels from the root scaffold only." Each shell keeps it next to the
theme (`ActiveLabels` on Android and iOS, `ACTIVE_LABELS` on web), so dialog code that never sees
the view can read it. A plain `cx.confirm` stays byte-identical on the wire, because the shell
applies the labels, not the core (ADR-0007).

The rule has held as the set of strings grew: #215 added the PDF and web-view strings, and #237
added `step_of` as a template with `{current}` / `{total}` placeholders rather than a fixed English
phrase. The barbershop demo sets its own wording ("Back to shop", "Show more", "Reload", "Sure",
"No thanks", "Pick"), and coffee sets none, which checks both paths at runtime.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app is fully localizable without editing shell code, in whatever language it
  chooses, and a new language needs no framework release.
- **Positive:** an app that sets nothing renders exactly as before.
- **Negative:** every new shell-drawn string is an ABI change: a new `ShellLabels` field, a minor
  release, and a template port (ADR-0008, ADR-0009). Forgetting it ships untranslatable English.
- **Negative:** one known gap today. The iOS date/time picker title falls back to "Pick a date" /
  "Pick a time" (`Core.swift`, `DateTimePlugin`), overridable per call through `Picker::title` but
  with no `ShellLabels` field.
- **Negative:** text the platform draws is outside this rule. Android picker month and day names
  follow the device locale, the browser's native date picker ignores the labels, and iOS system
  alerts keep their own layout (spec 2026-09-21, 2a and 2b).
- **Negative:** only the root scaffold's labels count. A nested scaffold cannot change them for its
  subtree.
- **Negative:** the precedence is implemented three times (Kotlin, Swift, Rust/web) and has no
  shared test, so a shell can drift unnoticed until review.
