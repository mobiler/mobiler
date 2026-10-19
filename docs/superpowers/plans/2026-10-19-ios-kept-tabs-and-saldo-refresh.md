# iOS kept tabs, demo theme fix, Saldo refresh — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** iOS keeps visited tab screens by ADR-0057's rules; coffee/todo/fullstack-todo get the Android theme fix; Saldo
becomes a CLI-managed app that equals the current template.

**Architecture:** PR 1 adds a `TabSlots`-style slot host to `ScaffoldView` in barbershop's `Render.swift`, ported to the
template, plus three one-file `Theme.kt` fixes. PR 2 regenerates Saldo's native shells with the current CLI, commits its
`.mobiler/` state, and guards it with a strict conformance test (ADR-0059).

**Tech Stack:** SwiftUI (iOS 17), Kotlin/Compose, Rust (`xtask` conformance tests, the `mobiler` CLI), GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-19-ios-kept-tabs-and-saldo-refresh-design.md`

## Global Constraints

- Both PRs ride the unreleased CLI **0.71.0**; no version bump, no library or ABI change.
- iOS deployment target 17.0 (ADR-0021): iOS 17 APIs with no availability checks.
- Barbershop first, template ported from its diff (ADR-0015). The template's drift from barbershop must be the same
  before and after (compare `diff` line counts with the package/name placeholders normalised).
- Merged ADRs are immutable: ADR-0057 is not edited. New ADRs: 0058 (iOS kept tabs), 0059 (Saldo CLI-managed).
- Every conformance test is proven by a deliberate mutation, recorded in the ADR's §4.
- Cargo: use `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler` for the root workspace; per-demo target
  dirs under `/media/zmilan/data2/` for demo builds.
- Commit messages with backticks: write them with `git commit -F <file>`.
- Before any `adb input`, check `adb shell dumpsys window | grep mCurrentFocus` (the maintainer may be using the phone).
  Never press Back twice. Stay-awake is on; turn it off at the end (`adb shell svc power stayon false`).
- PRs merge on green CI; the iPhone checks (TestFlight) follow, and nothing is tagged before both pass.

## Review Focus

1. **Colours on reveal (iOS):** dark mode or the palette flips while a tab is hidden; on return the hidden tab must
   show the new colours, not the old ones. Task 3 pins it with a structural assertion; the TestFlight check confirms.
2. **A route change inside a hidden tab:** the core resets a tab's stack while the user is elsewhere; the slot must
   rebuild, not show the old route. Task 3's host logic mirrors Android's; the reviewer checks it.
3. **A second tab bar layout (iPad rail):** the slot host must sit under both the bottom-tab and rail layouts
   (`mainColumn` is shared). Task 3 puts it in `mainColumn`'s body.
4. **Saldo shell edits silently lost:** a behaviour Saldo had only in its hand-edited shell. Task 7's diff report
   lists every difference with a ruling.
5. **Plugin anchor lines vs the strict test:** `plugin add` inserts lines above `mobiler:` anchors; the ADR-0059 test
   must accept exactly those and nothing else. Task 8's helper unit test pins both directions.

---

## PR 1 — branch `ios-kept-tabs`

### Task 1: Android theme fix in coffee, todo, fullstack-todo

**Files:**
- Modify: `demos/coffee/Android/app/src/main/java/dev/mobiler/coffee/ui/theme/Theme.kt`
- Modify: `demos/todo/Android/app/src/main/java/dev/mobiler/todo/ui/theme/Theme.kt`
- Modify: `demos/fullstack-todo/mobile/Android/app/src/main/java/dev/mobiler/mobile/ui/theme/Theme.kt`

**Interfaces:** none shared.

- [ ] **Step 1:** In each file, wrap the colour-scheme `when` in `remember(<every input it reads>) { … }`, and the
  typography and shapes likewise, as barbershop's `Theme.kt` lines 118–136 do. The keys are exactly the values each
  block reads (for example `theme, darkTheme, dynamicColor, context`); no other change.
- [ ] **Step 2:** Build one demo locally: `cd demos/coffee/Android && ./gradlew :app:assembleDebug` (its own
  `CARGO_TARGET_DIR` on data2). Expected: BUILD SUCCESSFUL. The other two are built by CI.
- [ ] **Step 3:** Commit: `fix(android): coffee, todo and fullstack-todo build their theme once per theme`.

### Task 2: ADR-0058 conformance test (RED)

**Files:**
- Modify: `xtask/tests/adr_conformance.rs`

**Interfaces:**
- Consumes: `root()`, `live_code(&str) -> String`, `fn_body(&str, &str) -> Option<&str>` (already in the file).
- Produces: the names Task 3 must use in Swift: `private struct TabSlots: View`, `func keptWhenHidden(`,
  `func holdsLiveView(`, `func holdsAny(`, `func tabSlotOf(`; inside `TabSlots`: the gate `if isActive { send(`,
  `.allowsHitTesting(`, `.accessibilityHidden(`, `resignFirstResponder`.

- [ ] **Step 1: Write the failing test** `adr_0058_ios_keeps_tab_screens_like_android` over
  `demos/barbershop/iOS/Sources/Render.swift` and `mobiler/templates/iOS/Sources/Render.swift`, on `live_code(text)`:
  - `fn_body(code, "private struct TabSlots")` exists ("ADR-0058: {path} has no TabSlots");
  - the host's body contains `keptWhenHidden(` ("must ask keptWhenHidden before keeping a hidden screen");
  - it contains `.allowsHitTesting(` and `.accessibilityHidden(` ("a hidden slot must not be tapped or read");
  - it contains `resignFirstResponder` ("a switch must end editing");
  - it contains `isActive` and `if isActive { send(` ("a hidden slot's send must be dropped"); `isActive` is read when
    the action is sent, not captured when the slot was built (Task 3 keeps it in a reference the closure reads);
  - `fn_body(code, "func holdsLiveView(")` names `.webView`, `.video`, `.map`, `.pdfView`;
  - `fn_body(code, "func keptWhenHidden(")` contains `holdsLiveView(`, `refreshing`, `.progress`, `.lazyList`,
    `.scaffold`, `.secure`.
  Check that `fn_body` finds a Swift `struct … {` body (its brace matcher is language-neutral); if not, ledger a ruling
  and add a Swift-aware lookup.
- [ ] **Step 2:** Run `CARGO_TARGET_DIR=… cargo test -p xtask --test adr_conformance adr_0058`. Expected: FAIL with
  "ADR-0058: demos/barbershop/iOS/Sources/Render.swift has no TabSlots".
- [ ] **Step 3:** Commit with Task 3 (the test lands green).

### Task 3: iOS `TabSlots` in barbershop's `Render.swift`

**Files:**
- Modify: `demos/barbershop/iOS/Sources/Render.swift` (`ScaffoldView.mainColumn`, around lines 1545–1591; new
  helpers after `ScaffoldView`)

**Interfaces:**
- Consumes: Task 2's names.
- Produces: `fileprivate func tabSlotOf(_ tabs: [SharedTypes.Tab]) -> Int?`,
  `fileprivate func keptWhenHidden(_ s: SharedTypes.Widget) -> Bool` (takes the `.scaffold` widget),
  `fileprivate func holdsLiveView(_ w: SharedTypes.Widget) -> Bool`,
  `fileprivate func holdsAny(_ w: SharedTypes.Widget, _ hit: (SharedTypes.Widget) -> Bool) -> Bool`,
  `private struct TabSlots: View`.

Design (the spec's A.1, A.2):
- [ ] **Step 1: Extract the body.** Move the existing body `Group { fill-mode VStack | ScrollView }` with its
  `.refreshableIf(onRefresh, send)` into `private struct ScreenBody: View` taking the screen's `content`, `refreshing`,
  `onRefresh` and `send`. Untabbed screens keep `ScreenBody(...).id(route).transition(navTransition)` exactly as today.
- [ ] **Step 2: The slot host.** When `tabSlotOf(tabs) != nil`, `mainColumn` renders `TabSlots` instead (same
  `.frame`, FAB and snackbar overlays). `TabSlots` gets the current screen's route, depth, content, refreshing,
  onRefresh, the selected slot, the tab count, `send`, and the `navTransition` inputs. It keeps `@State` storage:
  `[Int: Kept]` where `Kept` holds the last screen (route, depth, content, refreshing, onRefresh, the `.scaffold` widget
  for `keptWhenHidden`) and an `epoch: Int`; plus the last shown slot and the next epoch. On each render, the same
  steps as Android's `TabSlots`, applied in `onChange(of: slot)` / when the screen changes (never mutate `@State`
  during `body`): drop slots `>= tabs.count`; on a slot change, drop hidden slots that fail `keptWhenHidden`, drop the
  new slot if its kept route differs from the current route, end editing
  (`UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)`); store
  the current screen into the selected slot (new epoch if absent).
  Render `ZStack` over the sorted slot keys, each `ForEach` item `.id("\(i)-\(epoch)")`, each slot's content being
  `ScreenBody(...).id(kept.route).transition(navTransition)` inside its own animation scope (push/pop slides within
  the slot). The selected slot renders this frame's screen (not the stored copy, so it is never stale). Hidden slots:
  `.opacity(0)`, `.allowsHitTesting(false)`, `.accessibilityHidden(true)`. A tab switch must not animate: wrap the slot
  change in a `Transaction` with `disablesAnimations`, or scope `.animation(_, value: route)` so it doesn't fire across
  slots (the scaffold's `.animation(..., value: route)` would otherwise fade the switch; check the switch is instant).
- [ ] **Step 3: Gated send.** Each slot's `send` is `{ a in if isActive { send(a) } }` where `isActive` is read at send
  time (for example from a small `final class SlotGate { var active: Bool }` stored per slot and updated each render),
  so a closure captured by a hidden slot's views never sends.
- [ ] **Step 4: Colours on reveal (A.2).** A hidden slot was last rendered with the palette of that time. Give each
  slot the current `\.paletteRoles` environment and add the resolved palette identity (for example
  `ActivePalette.current` hashed, or `resolvedDark` plus `theme`) to the selected slot's identity inputs so a reveal
  after a theme change re-renders it. Verify by reading: every colour a body reads comes from `pal` (statics) or the
  environment; ledger a ruling on the mechanism chosen.
- [ ] **Step 5: Helpers.** `holdsAny` walks the same containers as `firstId` (row, column, box, grid, scroller,
  lazyList, card, a11y, swipeAction, split) plus `.scaffold` (body, sheet child, bottom bar). `holdsLiveView` hits
  `.webView`, `.video`, `.map`, `.pdfView`. `keptWhenHidden(.scaffold)` is false when the scaffold is `refreshing`,
  holds a live view, or its body holds a nested `.scaffold`, a `.progress(nil)`, a `.lazyList` with `loading ||
  refreshing`, or a `.textField` with `kind == .secure`. Doc comments mirror the Android ones and cite ADR-0058.
- [ ] **Step 6:** Run the Task 2 test filtered to barbershop (temporarily or by reading its message). Expected: the
  barbershop file passes; the template still fails "has no TabSlots".
- [ ] **Step 7:** Commit (test + barbershop): `feat(ios): keep visited tab screens like Android (ADR-0058)`.

### Task 4: Port to the template's `Render.swift`

**Files:** Modify `mobiler/templates/iOS/Sources/Render.swift`

- [ ] **Step 1:** Record the drift before: `diff demos/barbershop/iOS/Sources/Render.swift
  mobiler/templates/iOS/Sources/Render.swift | grep -c '^[<>]'` taken on the `main` versions (`git show main:…`).
- [ ] **Step 2:** Apply barbershop's diff (`git diff main -- demos/barbershop/iOS/Sources/Render.swift | patch` onto
  the template, fixing rejects by hand).
- [ ] **Step 3:** Drift after equals drift before. Expected: same count.
- [ ] **Step 4:** Run the full ADR-0058 test. Expected: PASS. Run `cargo test --workspace` (target dir on data2) to a
  log file; expected: all pass.
- [ ] **Step 5:** Commit: `feat(template): iOS keeps visited tab screens (ADR-0058)`.

### Task 5: ADR-0058, mutation proof, READMEs, spec

**Files:**
- Create: `docs/adr/ADR-0058-ios-keeps-visited-tab-screens.md` (format of ADR-0057; Deciding PRs: this PR's number)
- Modify: `docs/adr/index.md`
- Modify: READMEs matched by `git grep -n -i 'cross-fade\|crossfade\|scroll position\|tab switch' -- '*README.md'`
- Add: the spec and this plan

- [ ] **Step 1: Mutations.** Each in barbershop's `Render.swift`, run the test, see it fail with the named message,
  `git checkout -- <file>`, see it pass:
  (a) drop `.allowsHitTesting(false)`; (b) send through `send(a)` directly instead of the gate; (c) remove `.video`
  from `holdsLiveView`; (d) drop `.secure` from `keptWhenHidden`; (e) remove the `resignFirstResponder` call;
  (f) call `holdsLiveView` instead of `keptWhenHidden` in the host. Record each with its failure message in §4.
- [ ] **Step 2:** ADR-0058 text: context (parity asked by the maintainer; iOS had no speed problem), the rules (link
  ADR-0057, list the iOS mechanics: opacity 0 / hit testing / accessibility / resign first responder, slots still laid
  out unlike Android), the colours-on-reveal mechanism, consequences (memory, hidden slots still laid out, no fade on
  switch), the iOS theme finding (no counterpart of the Android theme bug, with the reason).
- [ ] **Step 3:** Run `cargo test -p xtask` (adr_docs + conformance). Expected: pass.
- [ ] **Step 4:** Commit: `docs(adr): ADR-0058 iOS keeps visited tab screens`.

### Task 6: Ship PR 1, Android spot check, TestFlight barbershop

- [ ] **Step 1:** Fresh code review + security review (opus subagents) on `git diff main...HEAD`; one fix pass,
  each fix test-first; deferred minors listed in the PR body.
- [ ] **Step 2:** Install the Task 1 coffee build on the A32 (`adb install -r`), open it, confirm it renders and
  typing in its field works. Uninstall afterwards only if it wasn't there before.
- [ ] **Step 3:** `ship-pr` skill: push, PR (body: what, the rulings, READMEs checked, iPhone checklist), all checks
  green (the three iOS lanes compile the new Swift), merge. If auto mode blocks the merge, give the maintainer the `!`
  command.
- [ ] **Step 4:** Dispatch `gh workflow run testflight-barbershop.yml` on `main` and tell the maintainer what to check
  on iPhone (spec A.5).

## PR 2 — branch `saldo-cli-managed` (after PR 1 merges)

### Task 7: Generate Saldo fresh and list its own edits

**Files:** scratch only (`<scratchpad>/saldo-fresh/`); report in the plan workspace (`saldo-diff.md`).

- [ ] **Step 1:** In the scratchpad: `cargo run -p mobiler -- new saldo --package rs.mobiler.saldo --display-name
  Saldo`, then in it `mobiler plugin add biometric`, `filepicker`, `files`, `sqlite`.
- [ ] **Step 2:** Diff every native file the fresh app has against `demos/saldo/` (Android tree, `iOS/Sources`,
  `iOS/project.yml`, `build-*.sh`, `.gitignore`s). Write `saldo-diff.md`: one row per difference — file, what, and a
  ruling: *template does it* (drop), *Saldo's own* (keep: icon, name, launch colours, any `mobiler.toml` config), or
  *framework improvement* (stop and ask the maintainer before PR 2 goes further).
- [ ] **Step 3:** Ledger the rulings. Expected known rows: release `signingConfig` (moves to the workflow, Task 10),
  `androidx.biometric` dependency (check the biometric plugin adds it through its manifest), exifinterface (template
  adds).

### Task 8: ADR-0059 conformance test (RED)

**Files:** Modify `xtask/tests/adr_conformance.rs`

**Interfaces:**
- Produces: `fn plugin_insertions_only(template: &str, app: &str) -> Result<(), String>`: `Ok` when `app` equals
  `template` except for blocks of extra lines each ending directly above a line containing `mobiler:` (an anchor)
  that is in both; `Err(first differing line)` otherwise.

- [ ] **Step 1: Unit test the helper** (`plugin_insertions_only_accepts_anchor_blocks_only`): accepts an extra block
  above an anchor; rejects an extra line elsewhere, a changed template line, a missing template line.
- [ ] **Step 2: The test** `adr_0059_saldo_equals_the_current_template`:
  - `demos/saldo/mobiler.toml` and `demos/saldo/.mobiler/base/` exist;
  - for every `.kt`, `.swift`, `.kts`, `.xml`, `.yml` file under `.mobiler/base/` that comes from the template:
    render the template file with Saldo's substitutions (the `{{PACKAGE}}`-family replacements, in
    `mobiler/src/templating.rs` order; package `rs.mobiler.saldo`, name `saldo`, the NDK version from the template's
    default) and assert the base copy equals it ("ADR-0059: Saldo's base is behind the template: run mobiler upgrade
    --apply in demos/saldo") and `plugin_insertions_only(rendered, saldo_file)` ("ADR-0059: {file} is edited by hand").
  Keep the substitution in the test small; if it can't reproduce a file exactly, ledger which and why.
- [ ] **Step 3:** Run it. Expected: FAIL "demos/saldo/mobiler.toml" missing.

### Task 9: Replace Saldo's shells (GREEN)

**Files:** `demos/saldo/` native trees, `demos/saldo/mobiler.toml`, `demos/saldo/.mobiler/`, `demos/saldo/.gitignore`

- [ ] **Step 1:** Copy the fresh app's native files over Saldo's (Android tree minus `generated/` and build output,
  `iOS/`, `mobiler.toml`, `.mobiler/`, `.gitignore`), then restore Saldo's own items from Task 7's *keep* rows
  (`ic_launcher*`, `Assets.xcassets` icon, display name, splash config). Keep `shared/`, `Cargo.toml`, `Cargo.lock`,
  `rust-toolchain.toml` (path deps stay). Delete Saldo files the fresh app doesn't have unless Task 7 kept them.
- [ ] **Step 2:** `cd demos/saldo && cargo run --manifest-path ../../mobiler/Cargo.toml -- upgrade`. Expected: nothing
  to do, nothing pending. `… -- doctor`: clean. Any upgrade finding on an in-repo demo is a CLI bug: fix it in
  `mobiler/src/` test-first, ledger it.
- [ ] **Step 3:** Run the ADR-0059 test. Expected: PASS. Run `cargo test --workspace`: pass.
- [ ] **Step 4:** Build Android locally: `cargo run -p mobiler -- build --no-install`… from `demos/saldo` (or
  `build-android.sh`), with a data2 target dir. Expected: APK built.
- [ ] **Step 5:** Commit: `chore(saldo): regenerate the shells with the current CLI; Saldo is CLI-managed`.

### Task 10: Signing moves to the APK workflow

**Files:** Modify `.github/workflows/android-apk-saldo.yml`

- [ ] **Step 1:** After `assembleRelease` (now unsigned: `app-release-unsigned.apk`), create a debug keystore with
  `keytool` if none, sign with `$ANDROID_HOME/build-tools/<latest>/apksigner sign`, zipalign first, and upload the signed
  `saldo-release.apk`. Check the TestFlight (saldo) workflow and ci.yml's Saldo lanes for paths the refresh moved;
  adjust.
- [ ] **Step 2:** Commit: `ci(saldo): sign the release APK in the workflow`.

### Task 11: ADR-0059, mutation proof, release skill

**Files:** Create `docs/adr/ADR-0059-saldo-is-cli-managed.md`; modify `docs/adr/index.md`,
`.claude/skills/release-cli/SKILL.md` (a step: "if the CLI PR changes a template shell, run `mobiler upgrade --apply`
in `demos/saldo` and commit it"); Saldo's mention in the root `README.md` if any.

- [ ] **Step 1: Mutations:** (a) add a comment line to Saldo's `MainActivity.kt` outside an anchor block → "edited by
  hand"; (b) change one line in the template's `Render.swift` without upgrading Saldo → "base is behind";
  (c) delete `demos/saldo/mobiler.toml`. Each fails, restore, passes. Record in §4.
- [ ] **Step 2:** `cargo test -p xtask`: pass. Commit: `docs(adr): ADR-0059 Saldo is CLI-managed`.

### Task 12: Device check, ship PR 2, TestFlight Saldo

- [ ] **Step 1:** A32: check the foreground app; `adb uninstall rs.mobiler.saldo` (approved: no data to keep);
  install the Task 9 APK. Check: launch, biometric unlock (ask the maintainer to touch the sensor), add and edit a
  bill, all four tabs with kept scroll, export a file, pick a file, language and dark mode, kill and relaunch (data
  persists).
- [ ] **Step 2:** Fresh code + security review; one fix pass; deferred minors in the PR body.
- [ ] **Step 3:** `ship-pr`; merge on green. Dispatch `testflight-saldo.yml` and the APK lane on `main`; tell the
  maintainer what to check on iPhone (spec B.4).
- [ ] **Step 4:** Turn stay-awake off; `pstree` for leftovers; update memory; then the 0.71.0 release waits for the
  maintainer's iPhone results and their OK to tag.
