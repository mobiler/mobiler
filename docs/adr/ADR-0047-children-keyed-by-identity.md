# ADR-0047: Native shells render a container's children under a stable identity — the widget's id, else its kind plus the first id inside it, numbered among siblings with the same base — never by position, so a widget appearing above another doesn't rebuild it

Status:        Accepted
Date decided:  2026-10-03
Deciding PRs:  #281
Supersedes:    none
Code anchor:   demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt (`ownId`, `firstId`, `childKeys`, `KeyedChildren`, the LazyList `key` and stay-at-top, the Scaffold body path), demos/barbershop/iOS/Sources/Render.swift (`ownId`, `firstId`, `childKeys`, `keyedChildren`, `KeyedChild`, `childViews`), and their template copies under mobiler/templates/
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
- **Without an id:** keyed by its kind plus the first id in its subtree, depth first. A `Row` holding the `email`
  field is `Row[email]`; a container with no id inside is just its kind.
- **Uniqueness:** every key ends in `#n`, its index among siblings with the same base, so keys are unique. A lazy list
  throws on a duplicate key.

Then inserting or removing a widget above another never changes the other's key, so it keeps its state. That holds
for a caption or error row, and for another row or card above the one holding a field.

### 2.1. Refutation Conditions

- **Condition 1: the reference shell and the template never walk a child list by position.** That covers Kotlin
  `children` / `kids` / `pinned` with `forEach` / `forEachIndexed` / `chunked`, a lazy list without a key, and Swift
  `ForEach(Array(children|bar.enumerated()), …)`. Both define `childKeys`.
  - **Validation Metric:** `adr_0047_shells_key_children_by_identity_not_position`.
- **Condition 2 (runtime):** on Android, a widget inserted above a focused field while typing leaves the field
  focused, with all the typed text. The inserted widget can be a text, a row above the field's row, or a widget above
  the field's card. Checked by the emulator probe recorded in PR #281.

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
- **Android:** `childKeys` builds the keys, and `KeyedChildren` wraps each child in `key(…)`.
  - **The Grid** keys its children within each chunked row. Compose can't move a keyed group between rows, so a
    child that changes rows is still rebuilt.
  - **The LazyList** passes the keys as `itemsIndexed(…, key = …)`. Keyed items keep the first visible one in place,
    so a list that was at the very top scrolls back to the top when its first key changes. Otherwise an item inserted
    there would land just above the viewport.
  - **The Scaffold body:** a `Column` body is walked through one path whether or not it holds a fill list. A results
    list appearing on a search screen doesn't rebuild the field above it.
- **iOS:** `keyedChildren` gives file-private `Identifiable` `KeyedChild` values to `ForEach`.
  - The Scaffold body still switches between a scrolling and a non-scrolling (fill) structure, because a fill list
    can't live inside a ScrollView. So a fill list appearing on iOS rebuilds the body, a known gap.
- **The kind** is the Kotlin class's simple name, or the Swift case name via `Mirror`. If an app turns on R8 class
  merging, Kotlin names can coincide. Keys stay unique through `#n`, but identity gets coarser.
- **Web:** mobiler-web rebuilds the view per render and handles fields itself; it is not covered here.

**Mutation proof:**
- Running the test on the Android shell without the fix (stashed for the emulator baseline) failed
  `adr_0047_shells_key_children_by_identity_not_position` ("has no childKeys").
- Restoring one positional loop (`widget.children.forEach { Render(it, send) }` in the Column arm) failed it.
- Restoring one Swift `ForEach(Array(children.enumerated()), id: \.offset)` failed it.
- Reverting restored green. The checks were later tightened: every Swift `ForEach` over `children`, `kids` or `bar`
  must use `keyedChildren`, and a Kotlin lazy list over children needs a `key =`. Both mutations still fail them, and
  so does dropping the LazyList's `key =`.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a caption, error row, banner, or another row or card appearing above a field no longer drops focus or
  typed text. Players and maps keep their state when the layout above them changes.
- **Positive:** a paged feed keeps its items' state when items of another kind, or with other ids, are inserted
  before them. Prepending same-kind, id-less items still shifts their numbering, as position did.
- **Negative (not new):** two same-base widgets swap state if the app reorders them. Give them ids where that matters
  (fields already have them).
- **Negative:** a widget whose id changes is rebuilt. That is the intended meaning of an id, but an app that derives
  ids from changing data (an index, a label) loses that widget's state. A step that replaces a focused field with a
  different-id field at the same place now drops the keyboard, where it used to keep it.
- **Negative:** the demos other than barbershop keep their own copies of the shells and stay positional until
  regenerated or upgraded.
- **Negative:** the web shell is unchanged.
