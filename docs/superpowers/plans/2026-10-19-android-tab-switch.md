# Android tab switching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Android tab switching fast: compile the shell's own code via a Baseline Profile, and keep each visited
tab's screen composed so a return visit is a diff, not a rebuild.

**Architecture:** Two Android-shell changes, built in `demos/barbershop` first and ported to the CLI template
(ADR-0015). §1 is a new `app/src/main/baseline-prof.txt` that AGP merges with the libraries' profiles. §2 replaces the
body's single route-keyed `AnimatedContent` with a tab-slot host: one composition per tab, a custom `Layout` that
measures only the active one, a send gate and focus clear for hidden ones, and no keeping of screens with live native
views. Ships as CLI 0.71.0; no Rust, library or ABI change.

**Tech Stack:** Kotlin 2.3 / Compose BOM 2026.01.01 / AGP 8.13.2 (strong skipping on by default), Rust xtask
conformance tests, the `mobiler` CLI template + `upgrade`.

**Spec:** `docs/superpowers/specs/2026-10-19-android-tab-switch-design.md`

## Global Constraints

- Android shell only: no change to `mobiler-ui`, `mobiler-core`, `mobiler-web`, `shared/` Rust or the ABI.
- Barbershop first, then the template, both in this one PR (ADR-0015). Other demos are not touched.
- Minimum API 26 (ADR-0039): no API call above 26 without a version guard.
- At most one kept screen per tab; a screen holding `Video`, `Map`, `WebView` or `PdfView` is never kept.
- A hidden slot sends no `Action`, holds no focus, is not measured, placed or drawn.
- A slot is never shown with any tree except the one the core sent this frame.
- Tab switches are instant (no fade); push/pop inside a tab keeps today's transition.
- CLI version 0.71.0. New record ADR-0057, with a conformance test proven by mutation.
- Build: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-barbershop`, `JAVA_HOME=~/jdk21`, build-tools 37.0.0.
- Phone (Samsung A32, `RF8R70603GD`): confirm with the user once before Task 1 Step 3 that the phone may be used. Before any `adb input`, check `adb shell dumpsys window | grep mCurrentFocus` shows barbershop;
  if another app (the firefighter game) is in front, stop and ask.
- Subagents: no `pgrep -f` or file-marker polling loops; wait on the command itself.

## Review Focus

1. **Returning to a tab whose stack the app changed while away** (reset to root, or tabs reordered): the user expects
   the new screen at once, no stale frame, no pop slide. → Task 3 Step 6, checks R1/R2.
2. **Keyboard open in tab A, switch to tab B:** keyboard closes; back on A no field is focused and typing into a field
   there still echoes correctly (FieldSync). → Task 3 Step 6, check F1.
3. **A `LazyList` with `on_load_more` in a hidden tab:** no load-more fires while hidden. → Task 3 Step 6, check L1.
4. **A tab with a video or map:** leaving it stops playback / map exactly as today; returning rebuilds it. → Task 3
   Step 6, check V1.
5. **Tabbed → untabbed → tabbed** (sign-out, a full-screen flow): the transition animates as today and nothing kept
   leaks into the untabbed screen. → Task 3 Step 6, check U1.

---

## File map

| File | Change |
|---|---|
| `demos/barbershop/Android/app/src/main/baseline-prof.txt` | new: shell profile rules |
| `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt` | tab-slot host, `holdsLiveView`, extracted `ScreenBody` / `navTransition` |
| `mobiler/templates/Android/app/src/main/baseline-prof.txt` | new: same rules with `{{PACKAGE_PATH}}` |
| `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt` | ported from barbershop's diff |
| `mobiler/src/upgrade.rs` (tests only) | the new file is offered/added by `upgrade` |
| `xtask/tests/adr_conformance.rs` | `adr_0057_android_keeps_tab_screens_measuring_only_the_active_one` |
| `docs/adr/ADR-0057-android-keeps-visited-tab-screens.md`, `docs/adr/index.md` | new record |
| `mobiler/Cargo.toml`, `Cargo.lock`, `NOTES.md` | 0.71.0 |

Scratch (not committed): `$SCRATCH/perf/` holding `build-release.sh`, `tabbench.sh`, results. `$SCRATCH` is the
session scratchpad.

---

### Task 1: Perf harness and the honest baseline (Run A)

**Files:**
- Create: `$SCRATCH/perf/build-release.sh`, `$SCRATCH/perf/tabbench.sh`, `$SCRATCH/perf/results.md`

**Interfaces:**
- Produces: `build-release.sh <out.apk>` (release, arm64, debug-key signed, from the current barbershop tree) and
  `tabbench.sh <label>` (appends one row to `results.md`). Tasks 2, 3 and 6 reuse both.

- [ ] **Step 1: Write `build-release.sh`.** In `demos/barbershop`:
  1. `cargo build -p shared --features uniffi`;
  2. `cargo run -p shared --bin codegen --features codegen,facet_typegen -- --language kotlin --output-dir Android/generated`;
  3. `./Android/gradlew -p Android :app:assembleRelease -PmobilerRustTargets=arm64`;
  4. `zipalign -f -p 4` then `apksigner sign --ks ~/.android/debug.keystore --ks-pass pass:android` into `$1`.

  Use the env from Global Constraints. `set -euo pipefail`.

- [ ] **Step 2: Write `tabbench.sh`.**
  1. Abort unless `mCurrentFocus` contains `dev.mobiler.barbershop`.
  2. Tap Home (100,2150), sleep 2.
  3. `dumpsys gfxinfo dev.mobiler.barbershop reset`.
  4. Four rounds of taps at x=320,540,760,100, y=2150, 1.5 s apart (never 980: Profile holds a WebView).
  5. Append `| $1 | janky % | p50 | p90 | p95 | p99 |` from `dumpsys gfxinfo` to `results.md`.

- [ ] **Step 3: Restore a clean phone.** Check `git status` is clean. Then:
  1. Build `A.apk`.
  2. `adb uninstall dev.mobiler.barbershop`, then `adb install A.apk`. This replaces the instrumented build left there
     on 2026-10-18.

- [ ] **Step 4: Compile like a Play install.** Launch the app once and wait 10 s, so profileinstaller writes the profile.
  Then run `adb shell cmd package compile -m speed-profile -f dev.mobiler.barbershop`,
  `adb shell am force-stop dev.mobiler.barbershop`, and relaunch.
  Expected: `adb shell dumpsys package dexopt | grep -A3 barbershop` shows `[status=speed-profile]`.

- [ ] **Step 5: Run A.** `tabbench.sh A-today` three times. Expected: three rows. Note the median in `results.md`.

- [ ] **Step 6: Run D (ceiling).** `cmd package compile -m speed -f`, force-stop, relaunch, then `tabbench.sh D-speed`
  ×3.

No commit (scratch only). Report A and D to the user before Task 2. If A is already within the spec's targets
(p90 ≤ 50 ms, janky < 25%), stop and ask whether §2 is still wanted.

---

### Task 2: Shell Baseline Profile (barbershop) and Run B

**Files:**
- Create: `demos/barbershop/Android/app/src/main/baseline-prof.txt`

**Interfaces:**
- Produces: the rule text that Task 4 copies to the template with `{{PACKAGE_PATH}}`.

- [ ] **Step 1: Write the file** with exactly these rules (spec §1), the package path substituted:

```
# Mobiler shell: compiled at install (Play) or at the next background dexopt.
HSPLdev/mobiler/barbershop/**;->**(**)**
Ldev/mobiler/barbershop/**;
HSPLcom/novi/**;->**(**)**
Lcom/novi/**;
```

- [ ] **Step 2: Verify the merge.** Build `B.apk` with `build-release.sh`. Then:
  `grep -c "dev/mobiler/barbershop" $(find demos/barbershop/Android/app/build/intermediates -path "*release*" -name "baseline-prof.txt" | grep -i merged)`.
  Expected: > 0, which proves AGP expanded the wildcards into the merged human-readable profile. If 0, the wildcard
  form is wrong: check AGP's output for a `baseline-prof.txt` parse warning before changing the rules.

- [ ] **Step 3: Run B.** Install `B.apk`, then repeat Task 1 Step 4 and run `tabbench.sh B-shellprof` three times.

- [ ] **Step 4: Decide.** If B's median p90 and janky % are not better than A's by more than run-to-run noise (the
  spread across A's three rows), delete the file. Record "§1 dropped: no gain" in `results.md` and the PR body.
  Otherwise keep it.

- [ ] **Step 5: Commit**

```bash
git checkout -b android-tab-switch
git add demos/barbershop/Android/app/src/main/baseline-prof.txt
git commit -F - <<'EOF'
perf(android): barbershop ships a Baseline Profile for the shell's own code

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3: Kept tab screens (barbershop) with the ADR-0057 conformance test

**Files:**
- Test: `xtask/tests/adr_conformance.rs`
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt`. The body is
  `AnimatedContent` at ~1663–1768. `holdsWebView` is at ~2453.

**Interfaces:**
- Produces, all `private` in `MainActivity.kt`:
  - `fun holdsLiveView(w: Widget): Boolean`: true when the tree holds a `WebView`, `Video`, `Map` or `PdfView`.
    It walks the same containers as `holdsWebView`. Implement both through one
    `fun holdsAny(w: Widget, hit: (Widget) -> Boolean): Boolean`.
  - `fun tabSlotOf(s: Widget.Scaffold): Int?`: the index of the one `selected` tab, null if there are no tabs or the
    selected count isn't exactly 1.
  - `fun AnimatedContentTransitionScope<Widget.Scaffold>.navTransition(): ContentTransform`: today's `transitionSpec`
    body, moved unchanged.
  - `@Composable fun ScreenBody(screen: Widget.Scaffold, padding: PaddingValues, send: (Action) -> Unit)`: today's
    `{ screen -> … }` lambda body, moved unchanged.
  - `@Composable fun TabSlots(screen: Widget.Scaffold, slot: Int, padding: PaddingValues, send: (Action) -> Unit)`:
    the host.
- Task 4 ports these names verbatim. Task 5's ADR cites them.

- [ ] **Step 1: Write the failing conformance test** in `xtask/tests/adr_conformance.rs`, beside the ADR-0047 test,
  over the same two Kotlin paths (barbershop + template):

```rust
#[test]
fn adr_0057_android_keeps_tab_screens_measuring_only_the_active_one() {
    let kotlin = [
        "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt",
        "mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt",
    ];
    for path in kotlin {
        let text = std::fs::read_to_string(root().join(path)).expect(path);
        let code: String = text.lines().map(kotlin_code).collect::<Vec<_>>().join("\n");
        let host = fn_body(&code, "private fun TabSlots(").unwrap_or_else(|| panic!("ADR-0057: {path} has no TabSlots"));
        assert!(host.contains("measurables[") && !host.contains("measurables.map"),
            "ADR-0057: {path} TabSlots must measure only the active slot");
        assert!(host.contains("if (isActive) send("), "ADR-0057: {path} a hidden slot's send must be dropped");
        assert!(host.contains("holdsLiveView("), "ADR-0057: {path} a screen with a live view must not be kept");
        assert!(host.contains("clearFocus("), "ADR-0057: {path} hiding a slot must clear focus");
        let live = fn_body(&code, "private fun holdsLiveView(").expect("holdsLiveView");
        for kind in ["Widget.WebView", "Widget.Video", "Widget.Map", "Widget.PdfView"] {
            assert!(live.contains(kind), "ADR-0057: {path} holdsLiveView misses {kind}");
        }
    }
}
```

  `fn_body(code, sig) -> Option<&str>`: the text from `sig` to its matching closing brace, found by brace counting from
  the first `{` after `sig`. Add it as a helper unless the file has one already (grep first). Add a unit test
  `fn_body_matches_nested_braces` with a two-level nested input.

- [ ] **Step 2: Run it to verify it fails.**
  `cargo test -p xtask --test adr_conformance adr_0057`. Expected: FAIL, "has no TabSlots".

- [ ] **Step 3: Extract without behaviour change.** Move the transition spec into `navTransition()` and the screen
  lambda into `ScreenBody(...)`. `AnimatedContent` calls them. Build `assembleDebug`. Expected: compiles, and the app
  behaves exactly as before (smoke: switch tabs, push/pop once on the emulator or phone).

- [ ] **Step 4: Implement `TabSlots`.** Rules, from spec §2:
  - **Outer switch.** The `AnimatedContent` call keeps `targetState = widget`. It gets
    `contentKey = { tabSlotOf(it)?.let { "\u0000tabs" } ?: it.route }`, so every tabbed screen shares one content
    key. Its content: `tabSlotOf(screen)?.let { TabSlots(screen, it, padding, send) } ?: ScreenBody(screen, padding, send)`.
  - **State.** `remember { mutableMapOf<Int, Slot>() }` with `private class Slot(var screen: Widget.Scaffold, val epoch: Int)`
    and a `remember`ed epoch counter. It is plain (non-snapshot) state, updated during composition: the host
    recomposes on every new `screen` anyway.
  - **On each composition:**
    1. Drop every slot whose index is `>= screen.tabs.size`.
    2. When the active slot changed since the last composition, drop every now-hidden slot whose screen
       `holdsLiveView`.
    3. For the active slot: if it is absent, create it with a new epoch. If its stored `screen.route != screen.route`
       and the slot was just revealed, replace it with a new epoch (rebuild, no transition). Otherwise set
       `slot.screen = screen`.
  - **Composition.** `Layout(content = { slots.toSortedMap().forEach { (i, s) -> key(i, s.epoch) { SlotContent(...) } } })`.
    The measure policy measures and places only the measurable at the active slot's position in that sorted order,
    with incoming constraints at (0,0). The others are never measured.
  - **`SlotContent`.**
    - Takes `isActive = i == slot`, held through `rememberUpdatedState`, as is `send`.
    - Builds the gated send once per slot: `remember { { a: Action -> if (isActive) send(a) } }`. Keep the exact text
      `if (isActive) send(` (the conformance test greps it).
    - Inside: `AnimatedContent(targetState = s.screen, contentKey = { it.route }, transitionSpec = { navTransition() })
      { ScreenBody(it, padding, gatedSend) }`.
  - **Focus.** `LaunchedEffect(slot) { focusManager.clearFocus(force = true) }` in `TabSlots`, with
    `focusManager = LocalFocusManager.current`. Any slot change drops focus and closes the keyboard.
  - **No fade.** A tab switch moves between slots, not inside one `AnimatedContent`, so it has no fade by construction.
    Don't add one.

  - **Effects audited** (spec §5), in barbershop's `Render`:
    - `LaunchedEffect(firstKey)` (~1350): LazyList scroll-to-top. It only runs on recomposition, and a hidden slot
      doesn't recompose.
    - `LaunchedEffect(widget.children.size, idx)` (~1362): load-more. It goes through the gated send.
    - The FieldSync `SideEffect`s (~1431/1450): local to the field, and they sync on reveal.
    - The Video/WebView/Map effects: never kept.
    - Add any effect the extraction surfaces to this list in the PR body.

- [ ] **Step 5: Run the conformance test.**
  `cargo test -p xtask --test adr_conformance adr_0057`. Expected: still FAILS, on the template path only
  (barbershop passes). Task 4 makes it green.

- [ ] **Step 6: Verify on the phone** (release build via `build-release.sh`, `speed-profile` as Task 1 Step 4). Check
  each item, and record pass/fail in `results.md`:
  - **S1:** tabs Home→Book→Shop→Home show the right screens; returning to a scrolled tab keeps its scroll.
  - **R1:** in a tab, push a detail screen, switch away, then fire that tab's root from the core. Use the barbershop
    path that resets a stack, if one exists; otherwise tap the same tab twice when the app pops-to-root on reselect.
    Back on the tab, the new screen shows at once with no slide.
  - **R2:** each tab still shows its own content after a cold restart (no slot holding another tab's screen).
  - **F1:** focus a field, keyboard up, switch tab: the keyboard closes. Return: nothing is focused. Type: the text
    echoes correctly, no doubled characters.
  - **L1:** a tab with a `LazyList` + load-more: switch away mid-list, wait 5 s.
    `adb logcat -s MobilerCore` (or the shell's action log) shows no `Fired(onLoadMore)` while hidden.
  - **V1:** the Profile tab (WebView) and any video/map screen: leave and return with no crash. A video stops on leave
    (no audio). Run `adb shell dumpsys meminfo dev.mobiler.barbershop` before and after one leave: WebView memory is
    not retained.
  - **U1:** if barbershop has an untabbed screen reachable from a tab (sheet excluded), go there and back: today's
    slide/fade still plays.
  - **T1:** TalkBack on: swiping through a tab never lands on another tab's content.
  - **W1:** on the wide emulator AVD (`mobiler_verify_p7`, landscape): the rail switches tabs the same way.

- [ ] **Step 7: Run C.** `tabbench.sh C-kept` ×3. Expected: return-visit p90 and janky % below B's medians. Rounds 2–4
  are all return visits.

- [ ] **Step 8: Commit** barbershop `MainActivity.kt` and `adr_conformance.rs` with a `perf(android): …` message via
  `git commit -F -`.

---

### Task 4: Port to the CLI template; upgrade adds the profile

**Files:**
- Create: `mobiler/templates/Android/app/src/main/baseline-prof.txt` (Task 2's text, with
  `dev/mobiler/barbershop` → `{{PACKAGE_PATH}}`). Skip it if Task 2 Step 4 dropped §1.
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt`
- Test: `mobiler/src/upgrade.rs` (tests module)

**Interfaces:**
- Consumes: Task 3's names (`TabSlots`, `SlotContent`, `ScreenBody`, `navTransition`, `tabSlotOf`, `holdsLiveView`,
  `holdsAny`, `Slot`).

- [ ] **Step 1: Write the failing test.** In `upgrade.rs`, next to the "missing shell file added" test (~2151), write
  `baseline_profile_is_a_shell_file_with_the_app_package`. Assert:
  - `classify(Path::new("Android/app/src/main/baseline-prof.txt"), b"x") == Class::Shell`;
  - after scaffolding a test app with package `dev.example.app` the way that test does, the file exists and contains
    `HSPLdev/example/app/**;->**(**)**`;
  - after deleting it, `upgrade --apply` recreates it.

- [ ] **Step 2: Run it.** `cargo test -p mobiler baseline_profile_is_a_shell_file`. Expected: FAIL (file absent).

- [ ] **Step 3: Add the template file.** Port the `MainActivity.kt` change as a patch: run `git diff` on barbershop's
  file, rename the package in the patch, then `git apply` it to the template. Resolve rejected hunks by hand. Keep
  template-only lines (`{{NAME}}Theme` etc.).

- [ ] **Step 4: Run the tests.** `cargo test -p mobiler baseline_profile_is_a_shell_file` and
  `cargo test -p xtask --test adr_conformance`. Expected: both PASS, including `adr_0057` on both paths.

- [ ] **Step 5: Scaffold check.**
  1. `cargo run -p mobiler -- new /tmp/tabcheck --package dev.example.tabcheck`, using the flags `mobiler new --help`
     shows.
  2. Build its Android debug APK with `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-tabcheck`.

  Expected: build OK. Then `rm -rf /tmp/tabcheck` and that target dir.

- [ ] **Step 6: Commit** the template, its test and its `baseline-prof.txt`.

---

### Task 5: ADR-0057, mutation proof, version, docs

**Files:**
- Create: `docs/adr/ADR-0057-android-keeps-visited-tab-screens.md` (from `docs/adr/TEMPLATE.md`)
- Modify: `docs/adr/index.md`, `mobiler/Cargo.toml` (0.70.2 → 0.71.0), `Cargo.lock`, `NOTES.md`

- [ ] **Step 1: Write the ADR.**
  - **Title claim:** "The Android shell keeps the last screen of each visited tab composed, measuring and drawing only
    the selected one; a hidden tab sends nothing and holds no focus, a screen holding a live native view is never
    kept, and a tab keeps its scroll position across a switch."
  - **Header:**
    - Code anchor: the two `MainActivity.kt` files (`TabSlots`).
    - Conformance:
      `xtask/tests/adr_conformance.rs (adr_0057_android_keeps_tab_screens_measuring_only_the_active_one)`.
    - Deciding PRs: the PR number, once the PR is open.
  - **§1:** the profile table from the spec, plus `results.md`'s A/B/C/D medians.
  - **§3 options:**
    - lazy body;
    - `SaveableStateHolder` (keeps state, not composition: still rebuilds);
    - keep everything, live views included (rejected per ADR-0026 lifecycle);
    - reset scroll on return (rejected: native convention).
  - **§5 Negatives:**
    - memory, up to one screen per tab;
    - iOS and web still reset scroll and rebuild (they should adopt this);
    - leaving tabs for an untabbed screen drops every kept slot;
    - a hidden slot's state is stale until revealed.

  Cite ADR-0026, ADR-0032, ADR-0047.

- [ ] **Step 2: Mutation proof.** Make three deliberate breaks, one at a time:
  - (a) measure every measurable (`measurables.map { it.measure(c) }`);
  - (b) pass `send` instead of the gated send;
  - (c) remove `Widget.Video` from `holdsLiveView`.

  Run `cargo test -p xtask --test adr_conformance adr_0057` after each. Expected: FAIL with the matching message each
  time. Revert, and the test is green again. Record all three in §4 under **Mutation proof:**.

- [ ] **Step 3: Index + docs checks.**
  1. Add the ADR-0057 row to `docs/adr/index.md`.
  2. Run `cargo test -p xtask --test adr_docs`. Expected: PASS.
  3. Grep every `README.md` for `tab`, `cross-fade`, `AnimatedContent`, `scroll position`, `baseline`. Update each
     match that describes tab-switch behaviour.
  4. Record the list of READMEs checked for the PR body.

- [ ] **Step 4: Version and full tests.** Bump the CLI to 0.71.0 and add the `NOTES.md` entry. Run
  `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** via `git commit -F -`.

---

### Task 6: Review, ship, release

- [ ] **Step 1: Reviews.** Get fresh code and security reviews (opus subagents) of the whole branch, both
  `MainActivity.kt` diffs side by side. Re-review after any fix pass.
- [ ] **Step 2: Ship.** Use the `ship-pr` skill. The PR body carries:
  - A/B/C/D;
  - the device checklist results;
  - the READMEs checked;
  - "§1 kept/dropped".

  Then fill ADR-0057's Deciding PRs with the PR number (before merge).
- [ ] **Step 3: Release.** Use the `release-cli` skill for v0.71.0. Confirm with the maintainer before the tag
  (irreversible). Then run `post-release`.
- [ ] **Step 4: Phone and memory cleanup.** Leave a normal release build of barbershop on the phone. Update the memory
  `android-tab-switch-perf.md` to RELEASED. Run `pstree` to check for leftover subagent shells.
