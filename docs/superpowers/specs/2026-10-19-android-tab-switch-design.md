# Android tab switching: shell profile + kept tab screens

**Asked by:** the maintainer, 2026-10-18/19: "plan for this big fix of slow tab switching" (skip the questions, profile
first). After profiling, they chose "design doc for both": a Baseline Profile and keeping visited tab screens composed.
**Release:** CLI 0.71.0 only. Android shell + template; no Rust, library or ABI change.
**Constrained by:**
- ADR-0001: the shell stays a generic renderer of the core's `Widget` tree; nothing here is app-specific.
- ADR-0005: the core's `Render` still drives every frame; a kept screen is never shown with a stale tree (§2.3).
- ADR-0015: barbershop's shell changes first; the template is ported from its diff in the same PR.
- ADR-0026: live native views (video, map, web view, PDF) are declaratively controlled. §2.4 keeps their lifecycle as
  today by never keeping a screen that holds one.
- ADR-0032: the shell holds no navigation state. The kept screens are a render cache keyed by what the core says (the
  selected tab and the screen's `route`); the shell never chooses what is shown and never shows a screen the core
  didn't send this frame.
- ADR-0039: minimum API 26. profileinstaller and wildcard profile rules work there.
- ADR-0046: `mobiler upgrade` adds the new template file (§1) under its normal rules: a review copy without
  `--apply`, written with it.
- ADR-0047: children keep their identity keys; kept screens add one level above the body, keyed by tab slot.

**New record:** ADR-0057, for the tab-screen cache rules (§2): which screens are kept, what a hidden one may do, and
that the scroll position survives a tab switch. It records a behaviour iOS and web should match later, so it is a
decision and not a PR detail.

## Why

Profile (barbershop release build, Samsung A32, Android 13, 2026-10-18), per tab switch:

| Stage | Time |
|---|---|
| Rust update + view + bridge | ~3–5 ms |
| Compose builds the new screen | ~50 ms |
| Measure and layout | ~45 ms |
| Draw record | ~60 ms |
| A later frame disposes the old screen | ~80 ms |

About 50% janky frames, p90 ~100 ms, p95 ~200 ms. The cost is all in Compose: every switch disposes the old screen and
builds the new one from nothing, because the body's `AnimatedContent` is keyed by `route`. Ruled out: the cross-fade,
the `Render` split, idle frames.

The code also runs largely interpreted (Nterp ~52% of the main thread). Forcing `speed` compilation cut main-thread work
~40% (p90 ~50 ms). The APK **already** carries a Baseline Profile (`assets/dexopt/baseline.prof`, from the Compose and
AndroidX libraries via profileinstaller), but nothing covers the shell's own code: `MainActivity.kt`'s `Render`, the
generated types, and the bincode/serde runtime. Those run on every frame of every switch.

## Goals

- Switching to a tab visited before is a recomposition diff, not a rebuild, and disposes nothing.
- The shell's own code is in the app's Baseline Profile, so an app installed from Play (or after the device's
  background dexopt) runs it compiled.
- Measured on the A32 (method in §4): janky frames and p90 clearly down from the baseline. Targets: p90 ≤ 50 ms on
  return visits, janky < 25%.
- Every app gets both with `mobiler upgrade --apply`. No app code change.

## Non-goals

- **A lazy page body** (`LazyColumn` instead of `Column` + `verticalScroll`). It would also cut the first visit, but it
  risks keyboard/focus/scroll (0.62/0.63) and child identity (ADR-0047). It's a follow-up only if §4 still shows a slow
  first visit.
- **R8 in the template** (`isMinifyEnabled = true`). Compose recommends it, but JNA/UniFFI reflection needs keep rules
  that every plugin would have to get right. Separate decision.
- **iOS and web.** iOS is smooth on its hardware; web isn't affected. ADR-0057 names the behaviour they should adopt.
- **A Macrobenchmark-generated profile.** It needs a benchmark module and a device in CI. Wildcard rules over the
  shell's packages cover what we need.

## Anti-goals

- A hidden screen that sends an `Action`, keeps focus or the keyboard, or is reachable by TalkBack.
- A screen shown with any tree except the one the core sent this frame.
- A kept video still playing, a kept map still rendering, or a kept web view holding memory in the background.
- Unbounded memory: at most one kept screen per tab.

## 1. Shell Baseline Profile

A new template file, `Android/app/src/main/baseline-prof.txt` (Shell class, so `upgrade` adds and refreshes it):

```
# Mobiler shell: compiled at install (Play) or at the next background dexopt.
HSPL{{PACKAGE_PATH}}/**;->**(**)**
L{{PACKAGE_PATH}}/**;
HSPLcom/novi/**;->**(**)**
Lcom/novi/**;
```

- `{{PACKAGE_PATH}}` covers `MainActivity`, `Core`, the plugins, and the generated `.shared` / `.shared.types`
  (they live under the app package). `com/novi` is the generated bincode/serde runtime.
- The UniFFI glue is in `{{PACKAGE_PATH}}/shared`, so it's covered. JNA is not: its hot path is native.
- AGP merges this with the libraries' profiles into `assets/dexopt/baseline.prof`. No Gradle change: profileinstaller
  is already on the classpath through Compose.
- Wildcards in an app's `baseline-prof.txt` are supported by AGP 8+. The plan verifies the merged profile really
  contains the shell's classes (`profgen`/`apkanalyzer`), not just that the build passes.
- **Measure before keeping it** (§4). If the shell profile adds nothing over the libraries' profile, drop §1 and say so
  in the PR.

## 2. Kept tab screens

### 2.1 Which screens are kept

The body host gets a **tab slot** for each screen that has tabs with exactly one `selected`: the slot is that tab's
index. A slot holds one composition: the last screen the core showed in that tab.

- Screens without tabs (login, onboarding, a full-screen flow) render exactly as today, through the existing
  `AnimatedContent`.
- Moving between a tabbed and an untabbed screen also animates as today. The outer `AnimatedContent` is keyed by
  "tabbed" or by `route`, so the whole tab host enters and leaves as one screen.
- Inside a slot, a `route` change (push/pop within the tab) animates as today: each slot has its own
  `AnimatedContent` with the same transition spec.

### 2.2 Switching

- A switch to another slot shows that slot at once (no cross-fade; tab switches already skip it beside a web view).
- The slots live in one custom `Layout` that measures and places **only the active slot**. A hidden slot stays
  composed but is not measured, placed or drawn, so it costs no layout or draw time. It doesn't recompose either: its
  input doesn't change while hidden.
- The old slot is not disposed, which takes the ~80 ms disposal off the switch.
- A tab visited for the first time is built as today. The cache only helps return visits; first visits are where §1
  and the follow-up apply.

### 2.3 Never stale, never active

- On reveal the slot is given the screen the core just sent. If its `route` equals the route the slot holds, that's a
  normal recomposition diff. If it differs (the app reset that tab's stack while it was away), the slot is rebuilt
  without a transition, which costs what a switch costs today.
- While hidden, the slot's `send` drops every `Action` (no "load more" from a hidden `LazyList`, no field echo).
- On hide, the shell clears focus inside the slot, so the keyboard closes and no hidden field holds focus.
- Unplaced nodes are not in the semantics tree, so TalkBack can't reach a hidden slot. The plan verifies this on a
  device.

### 2.4 Live native views are never kept

A screen holding a `Video`, `Map`, `WebView` or `PdfView` (anywhere in its tree: `holdsWebView` widens to
`holdsLiveView`) is disposed when hidden, exactly as today. Its player, map and web view release as they do now, and
ADR-0026's command semantics are untouched. Returning to it rebuilds it.

### 2.5 What an app sees

- Returning to a tab keeps its scroll position (a native Android convention; today it resets to the top).
- `remember`ed shell state inside the screen survives: an expanded `Disclosure`, a scroller's offset, a field's
  cursor.
- Tab switches no longer cross-fade on Android.
- Memory: up to one extra composed screen per tab (usually 3–5), without live native views.
- Nothing else. The core's model, routes and events are unchanged.

## 3. Upgrade and docs

- The template change is `MainActivity.kt` (Shell + the existing anchors; 3-way merged) plus the new
  `baseline-prof.txt`.
- READMEs: grep for tab / `AnimatedContent` / cross-fade / scroll-position wording in the root `README.md`,
  `mobiler/README.md` and `mobiler-ui/README.md` (the `Scaffold` tabs docs) and update each match.
- ADR-0057 with a conformance test in `xtask/tests/adr_conformance.rs`: the template's `MainActivity.kt` measures only
  the active slot and drops a hidden slot's `send`. The test is proven to fail by a mutation, recorded in the ADR.

## 4. Measurement

Same device, same build recipe as the profile (release, signed with the debug key, arm64), same `tabbench`: Home, reset
gfxinfo, four rounds over the four non-web tabs 1.5 s apart, read janky % and p50/p90/p95/p99.

| Run | Compilation | What it shows |
|---|---|---|
| A: today | `speed-profile` with today's profile | baseline as a Play user gets it |
| B: + §1 | `speed-profile` with the shell profile | §1's gain |
| C: + §1 + §2 | as B | §2's gain on return visits |
| D: ceiling | `speed` | how far §1 is from full compilation |

`speed-profile` after first launch (`cmd package compile -m speed-profile -f`) stands in for a Play install. Run A
first; it is the honest baseline, not the uncompiled numbers above.

## 5. Risks

- **A hidden slot's effects.** A `LaunchedEffect` in a hidden slot still runs (composition is live). Today's effects
  either `send` (dropped while hidden) or animate (no frame while unplaced). The plan lists every `LaunchedEffect` /
  `DisposableEffect` in `Render` and checks each.
- **Snackbar host count.** The `SnackbarBus.hosts` counter sits outside the body, so slots don't change it.
- **Field render-first sync** (FieldSync): a hidden field doesn't see core updates until revealed, then syncs as on
  any update. The plan tests typing in tab A, switching away and back.
- **Wide layout** (navigation rail): the same slot host, since the body is shared.

## Amendment 2026-10-19 (during the build)

Measured on the A32 (numbers in the PR body and ADR-0057 §1):

- **§1 dropped.** A fresh install with only the shipped Baseline Profile (installed with profileinstaller's broadcast,
  Macrobenchmark's Partial mode) measured the same as full `speed` compilation: p90 48 vs 53 ms, janky ~51% vs ~49%.
  So a shell profile, bounded by `speed`, can't gain. The earlier "uncompiled" numbers came from installing with
  `adb`.
- **A root cause the spec missed: the theme.** Kept screens alone gained nothing. A revealed tab still recomposed in
  full (108–181 ms), because the Android theme built a new `ColorScheme` on every core update, and `MaterialTheme`
  hands it down through a static local, so the whole app recomposed on every update. `Theme.kt` now remembers the
  scheme, typography and shapes per distinct theme (a bug fix, so no ADR; in the PR).
- **Stable generated types** (`compose_stability.conf`) were tried and dropped: no measurable difference.
- **Result** (theme fix + kept screens): p90 48 → 38 ms, p95 129 → 61 ms, janky 51% → 43%, and the longest frame on a
  return switch went from ~140–190 ms to ~40 ms. Most of the frames still counted as janky are the tab bar's
  indicator animation. At 90 Hz they are GPU-bound (10–20 ms of draw each, with under 2 ms of UI work), so they're
  out of scope here. The spec's janky < 25% target is not met for that reason.
