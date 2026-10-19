# ADR-0057: The Android shell keeps the last screen of each visited tab composed and measures, places and draws only the selected one; a hidden tab sends nothing and holds no focus, a screen holding a live native view, a running indeterminate animation, a nested Scaffold or a secure field is never kept, and a tab keeps its scroll position across a switch

Status:        Accepted
Date decided:  2026-10-19
Deciding PRs:  #302
Supersedes:    none
Code anchor:   demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt (TabSlots, tabSlotOf, keptWhenHidden, holdsLiveView, holdsAny, ScreenBody, navTransition), mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt (the same)
Conformance:   xtask/tests/adr_conformance.rs::adr_0057_android_keeps_tab_screens_measuring_only_the_active_one

## 1. Context (The Problem)

Tab switching on Android was slow on a mid-range phone (Samsung A32, Android 13, 90 Hz). The profile of a barbershop
release build (2026-10-18) showed this per switch:

| Stage | Time |
|---|---|
| Rust update, view and bridge | ~3–5 ms |
| Compose builds the new screen | ~50 ms |
| Measure and layout | ~45 ms |
| Draw record | ~60 ms |
| A later frame disposes of the old screen | ~80 ms |

The body was one `AnimatedContent` keyed by the screen's `route`. Every tab switch disposed of the old screen and built
the new one from nothing, including its images, cards and charts.

A second cause was found while building this (2026-10-19). The Android theme built a new `ColorScheme` on every core
update. `MaterialTheme` hands it down through a static composition local, and `ColorScheme` compares by identity, so
every update recomposed the whole app. With that cause in place, kept screens were worthless: a revealed tab
recomposed in full anyway (108–181 ms). The theme fix shipped in the same PR as a bug fix. This record decides only
the kept screens.

Compilation was ruled out as a lever. A fresh install with only the shipped Baseline Profile (the Compose libraries'
own, via profileinstaller) measured the same as a fully `speed`-compiled app.

## 2. Hypothesis

The shell keeps one composition per tab slot (the selected tab's index), holding the last screen the core showed in
that tab. A custom `Layout` measures and places only the selected slot. If so, then:
- a return to a tab is a small recomposition of the core's new tree into a live composition: no rebuild, and no
  disposal on the switch;
- a hidden slot costs no frame time;
- the app can't tell, except that a tab keeps its scroll position and remembered shell state.

These rules keep it safe:
- A hidden slot's `send` drops every `Action`.
- A switch clears focus, so the keyboard closes.
- A hidden slot is unplaced, so it is not in the accessibility tree.
- A screen is let go once hidden unless `keptWhenHidden` allows it. Not kept:
  - a screen holding a `WebView`, `Video`, `Map` or `PdfView`, so live native views keep their lifecycle
    (ADR-0026);
  - a screen still animating: `refreshing`, an indeterminate `Progress`, or a `LazyList` that is `loading` or
    `refreshing`. Their infinite transitions ask for a frame on every vsync, placed or not;
  - a screen nesting a `Scaffold`, whose `BackHandler` and sheet outlive placement;
  - a screen with a SECURE `TextField`, whose text would otherwise stay in memory after the user left.
- A slot whose tab moved to another `route` while it was away is rebuilt without a transition, never shown stale.

### 2.1. Refutation Conditions

- **Condition 1: only the selected slot is measured.** The host indexes `measurables[...]` and never maps or loops over
  them.
  - **Validation Metric:** `adr_0057_android_keeps_tab_screens_measuring_only_the_active_one`.
- **Condition 2: a hidden slot sends nothing**, and a switch clears focus.
  - **Validation Metric:** the same test (`if (isActive) send(`, `clearFocus(`). On a device: uiautomator shows the
    field `focused="false"` after a return; text typed on another tab never reaches the hidden field.
- **Condition 3: what must not run hidden is never kept.** `holdsLiveView` names all four live kinds;
  `keptWhenHidden` checks it plus `refreshing`, indeterminate `Progress`, `LazyList.loading`, a nested `Scaffold`
  and `FieldKind.SECURE`; the host asks `keptWhenHidden`.
  - **Validation Metric:** the same test. On a device: Profile (WebView) and Home round trips with no renderer crash,
    and 0 frames rendered while idle after a tab round trip.
- **Condition 4: it is faster.** On the A32 (same build recipe, shipped profile only):
  - p90 frame time 48 → 38 ms; p95 129 → 61 ms; janky frames 51% → 43%;
  - the longest frame of a return switch ~140–190 ms → ~40 ms.

  Without kept screens, with the theme fix alone, the switch frame stayed at 91–94 ms.
  - **Validation Metric:** the gfxinfo/perfetto numbers in the deciding PR.

## 3. Considered Options & Rationale for Refutation

- **Option A: a Baseline Profile for the shell's code** `[recorded: docs/superpowers/specs/2026-10-19-android-tab-switch-design.md §1; PR #302 body]`
  Rejected after measurement. Full `speed` compilation was no faster than the shipped library profile, so a shell
  profile can't gain.
- **Option B: `SaveableStateHolder` per tab** `[reconstructed]`
  Rejected. It keeps saved state (the scroll offset), not the composition, so every switch still rebuilds and
  disposes the screen.
- **Option C: a lazy page body (`LazyColumn`)** `[recorded: the design spec, Non-goals]`
  Deferred. It would also shorten first visits, but it risks the keyboard and focus work (CLI 0.62/0.63) and child
  identity (ADR-0047).
- **Option D: keep every screen, live views included** `[reconstructed]`
  Rejected. A hidden video would keep playing and a hidden web view would hold its memory, which needs pause/resume
  rules the declarative live-view contract (ADR-0026) doesn't have.
- **Option E: keep screens but reset scroll on return** `[recorded: the design spec §2.5, approved by the maintainer 2026-10-19]`
  Rejected. Returning to a tab where you left it is the native convention on Android and iOS.
- **Option F: mark the generated types stable** (`compose_stability.conf`) `[recorded: PR #302 body]`
  Tried and dropped. The kept screens and the theme fix performed the same with it and without it.

## 4. Decision & Rationale for Corroboration

Option: kept tab slots, in a `TabSlots` host under the Scaffold's `AnimatedContent`. Every screen with exactly one
selected tab shares one content key there, so a tab switch happens inside `TabSlots`, at once, with no fade. A
push/pop inside a tab keeps the existing slide. A move between a tabbed and an untabbed screen animates as before.

ADR-0032 holds. The slots are a render cache keyed by what the core sent (the selected tab and the screen's
`route`). The shell never chooses which screen to show, and never shows a slot with any tree except this frame's.

**Mutation proof:** the invariant was broken three times on purpose in barbershop's `MainActivity.kt`. The test failed
each time; each file was then restored with `git checkout -- <file>`, and the test passed again.
- The measure policy measured every child (`measurables.map { it.measure(constraints) }`). The test failed with
  `ADR-0057: demos/barbershop/…/MainActivity.kt TabSlots must measure only the active slot`.
- The slot passed `send` instead of the gated send (`{ a: Action -> send(a) }`). The test failed with
  `ADR-0057: … a hidden slot's send must be dropped`.
- `it is Widget.Video ||` was removed from `holdsLiveView`. The test failed with
  `ADR-0057: … holdsLiveView misses Widget.Video`.

After the review pass the test was tightened, and four more breaks each failed it:
- the SECURE clause was dropped from `keptWhenHidden` (`… keptWhenHidden misses FieldKind.SECURE`);
- the host called `holdsLiveView` instead of `keptWhenHidden`
  (`… TabSlots must ask keptWhenHidden before keeping a hidden screen`);
- the measure policy showed `measurables[0]` (`… must measure only the active slot`);
- the gate was frozen with `val isActive = remember { i == slot }` (`… a hidden slot's send must be dropped`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** returning to a visited tab costs a small recomposition, and the old screen is no longer disposed on the
  switch.
- **Positive:** a tab keeps its scroll position and an open `Disclosure`, including inside a pushed detail screen
  that the core still shows (the core keeps the stack; the shell keeps only the composition).
- **Negative:** memory. Up to one extra composed screen per tab stays alive (live native views excluded).
- **Negative:** a hidden screen's coroutines are not paused. A new kind of continuous work in a widget (an
  infinite animation, a polling loop) must be added to `keptWhenHidden`, or it keeps running while hidden.
- **Negative:** a first visit to a tab still builds the screen from nothing. Only a lazy body (Option C) would cut that.
- **Negative:** moving to an untabbed screen and back drops every kept slot, since the tab host leaves the composition.
- **Negative:** a hidden slot holds the tree it last showed until it is revealed. Code that reads a hidden slot would
  see stale data; nothing does today.
- **Negative:** iOS and the web shell still rebuild on every switch and reset the scroll position. Until they follow
  this record, Android and iOS differ there.
- **Negative:** tab switches no longer cross-fade on Android.
