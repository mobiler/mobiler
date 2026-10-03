# ADR-0047: Native shells render a container's children under a stable identity — the widget's id when it has one, else its kind and its place among siblings of that kind, always unique — never by position, so a widget appearing above another doesn't rebuild it

Status:        Accepted
Date decided:  2026-10-03
Deciding PRs:  #281
Supersedes:    none
Code anchor:   demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt (`childKeys`, `KeyedChildren`, the LazyList `key`), demos/barbershop/iOS/Sources/Render.swift (`childKeys`, `keyedChildren`, `KeyedChild`, `childViews`), and their template copies under mobiler/templates/
Conformance:   xtask/tests/adr_conformance.rs::adr_0047_shells_key_children_by_identity_not_position

## 1. Context (The Problem)

Compose and SwiftUI keep a view's state (a text field's text, cursor and focus; a player; a map's camera) as long as
the view keeps its identity. The Android shell rendered a container's children in a plain loop, and the iOS shell
used `ForEach(..., id: \.offset)`. Either way, a child's identity was its position.

An investigation of the appointments team's "doubled text" report (2026-10-03) found the consequence on an Android 8
emulator. The app inserted a widget above a focused text field once the field was non-empty, such as a caption or a
validation row. The field was rebuilt: it lost focus after the first character, and the rest of the typing went
nowhere. Any screen whose layout changes above a field while the user types hits this, and it also restarts players
and maps.

## 2. Hypothesis

Key each child the same way on every shell:
- **With an id:** a widget that has one (`TextField`, `SearchField`, `Toggle`, `Checkbox`, `Slider`, `Video`, `Map`)
  is keyed by that id.
- **Without an id:** keyed by its kind and its index among siblings of the same kind.
- **Uniqueness:** a repeated key gets `#n`. A lazy list throws on a duplicate key.

Then inserting or removing a widget of another kind above a widget never changes that widget's key, so it keeps its
state.

### 2.1. Refutation Conditions

- **Condition 1: the reference shell and the template never walk a child list by position.** That covers Kotlin
  `children` / `kids` / `pinned` with `forEach` / `forEachIndexed` / `chunked`, a lazy list without a key, and Swift
  `ForEach(Array(children|bar.enumerated()), …)`. Both define `childKeys`.
  - **Validation Metric:** `adr_0047_shells_key_children_by_identity_not_position`.
- **Condition 2 (runtime):** on Android, a widget inserted above a focused field while typing leaves the field
  focused, with all the typed text. Checked by the emulator probe recorded in PR #281.

## 3. Considered Options & Rationale for Refutation

- **Option A: key by position (the old behaviour)** `[recorded: the shells before #281]`
  Rejected: inserting anything above a widget rebuilds it.
- **Option B: key only widgets that have an id; leave the rest positional** `[reconstructed]`
  Rejected. A field inside a `Card` or `Column` (no id) is rebuilt when its container shifts, so the common case, a
  form inside a card, still breaks.
- **Option C: give every widget an id in the ABI** `[reconstructed]`
  Rejected for now. It is an ABI change (ADR-0008) and a burden on every app, and kind-plus-ordinal covers the
  common layout changes without it.
- **Option D: id, else kind and ordinal, made unique** `[recorded: PR #281]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D, in barbershop first and ported to the template (ADR-0015).
- **Android:** `childKeys` builds the keys. `KeyedChildren` wraps each child in `key(…)`. The Grid uses the keys
  across its chunked rows, and the LazyList passes them as `itemsIndexed(…, key = …)`.
- **iOS:** `keyedChildren` gives `Identifiable` `KeyedChild` values to `ForEach`.
- **The kind** is the Kotlin class's simple name, or the Swift case name via `Mirror`.
- **Web:** mobiler-web rebuilds the view per render and handles fields itself; it is not covered here.

**Mutation proof:**
- Running the test on the Android shell without the fix (stashed for the emulator baseline) failed
  `adr_0047_shells_key_children_by_identity_not_position` ("has no childKeys").
- Restoring one positional loop (`widget.children.forEach { Render(it, send) }` in the Column arm) failed it.
- Restoring one Swift `ForEach(Array(children.enumerated()), id: \.offset)` failed it.
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a caption, error row or banner appearing above a field no longer drops focus or typed text. Players
  and maps keep their state when the layout above them changes.
- **Positive:** a paged feed keeps its items' state when items are inserted before them.
- **Negative:** two widgets of the same kind swap state if the app reorders them, because their ordinals swap. Give
  them ids where that matters (fields already have them).
- **Negative:** a widget whose id changes is rebuilt. That is the intended meaning of an id, but an app that derives
  ids from changing data (an index, a label) loses that widget's state.
- **Negative:** the demos other than barbershop keep their own copies of the shells and stay positional until
  regenerated or upgraded.
- **Negative:** the web shell is unchanged.
