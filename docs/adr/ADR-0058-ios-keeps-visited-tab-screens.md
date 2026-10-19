# ADR-0058: The iOS shell keeps the last screen of each visited tab by ADR-0057's rules: a hidden tab is invisible, can't be tapped, is hidden from VoiceOver and sends nothing, a switch ends editing and doesn't animate, the same screens are never kept, and a tab keeps its scroll position across a switch

Status:        Accepted
Date decided:  2026-10-19
Deciding PRs:  #303
Supersedes:    none
Code anchor:   demos/barbershop/iOS/Sources/Render.swift (TabSlots, SlotCache, SlotGate, TabScreen, ScreenBody, tabSlotOf, keptWhenHidden, holdsLiveView, holdsAny), mobiler/templates/iOS/Sources/Render.swift (the same)
Conformance:   xtask/tests/adr_conformance.rs::adr_0058_ios_keeps_tab_screens_like_android

## 1. Context (The Problem)

ADR-0057 made the Android shell keep each visited tab's screen, so a return keeps its scroll position. The iOS shell
still keyed the whole body by `route`: every tab switch dropped the old screen, built the new one, reset the scroll to
the top and cross-faded. iOS was never slow at this. The maintainer asked for parity (2026-10-19: "just that they behave
same"), and ADR-0057 §5 named the difference as a negative to close.

## 2. Hypothesis

The iOS shell keeps one slot per visited tab, holding the last screen the core showed in that tab, by the rules of
ADR-0057. If so, a return to a tab finds it as it was left, and the app can't tell, except for the kept scroll position
and shell state. The two shells then behave the same.

In iOS terms:
- `TabSlots` renders every kept slot in a `ZStack`. The selected slot shows the screen the core just sent. A hidden slot
  is `opacity(0)`, `allowsHitTesting(false)` and `accessibilityHidden(true)`.
- A hidden slot's `send` drops every `Action`. Each slot's closure reads a `SlotGate` when the action is sent, so a
  closure captured while the slot was shown can't send after it is hidden.
- A switch resigns the first responder, so the keyboard closes, and `.transaction(value: slot)` clears its animation,
  so it neither fades nor slides. A push/pop inside one slot slides as before (each slot's content is keyed by its
  route).
- `SlotCache` (a plain class mutated during `body`, the twin of Android's remembered map) lets a screen go once hidden
  unless `keptWhenHidden` allows it, with ADR-0057's list: a live native view (`WebView`, `Video`, `Map`, `PdfView`);
  `refreshing`, an indeterminate `Progress`, or a `LazyList` that is `loading` or `refreshing`; a nested `Scaffold`; a
  SECURE `TextField`. A slot whose tab moved to another `route` while it was away is rebuilt without a transition.

Unlike Android, a hidden slot is still laid out (SwiftUI has no unmeasured child) and still re-renders on each core
update (its gated closure is new each render). This is what keeps its colours current after a theme or light/dark flip
while it is hidden: every colour comes from statics `Core.swift` sets on each update, or from `\.paletteRoles`.

iOS has no counterpart of the Android theme bug fixed beside ADR-0057: there is no `MaterialTheme` static local that
recomposes the app on a new scheme object. The palette and theme are plain statics read during rendering.

### 2.1. Refutation Conditions

- **Condition 1: a hidden slot is unreachable.** It can't be tapped or read, and its `send` is gated.
  - **Validation Metric:** `adr_0058_ios_keeps_tab_screens_like_android` (`.allowsHitTesting(`,
    `.accessibilityHidden(`, `if gate.active { send(`). On an iPhone: VoiceOver doesn't reach a hidden tab.
- **Condition 2: a switch ends editing.**
  - **Validation Metric:** the same test (`resignFirstResponder`). On an iPhone: the keyboard closes on a switch.
- **Condition 3: what must not run hidden is never kept.** `holdsLiveView` names all four live kinds; `keptWhenHidden`
  checks it plus `refreshing`, indeterminate `.progress`, `.lazyList` loading, a nested `.scaffold` and `.secure`;
  `SlotCache` asks `keptWhenHidden`.
  - **Validation Metric:** the same test. On an iPhone: Profile's web view and a video behave as before.
- **Condition 4: a return keeps the scroll position, with current colours.**
  - **Validation Metric:** the maintainer's TestFlight check (barbershop): scroll a tab, switch, return; flip dark mode
    while a tab is hidden, return.

## 3. Considered Options & Rationale for Refutation

- **Option A: SwiftUI's `TabView`** `[reconstructed]`
  Rejected. It keeps tabs alive, but it owns the selection and draws its own tab bar. The core owns which tab is
  selected (ADR-0032), and the shell's tab bar and rail are themed and badged (ADR-0050).
- **Option B: remember only the scroll offset per tab** `[reconstructed]`
  Rejected. It restores the offset but rebuilds the screen, so an open `Disclosure` or a field's state is lost, and it
  differs from Android, which keeps the whole screen.
- **Option C: keep the cross-fade on iOS** `[recorded: docs/superpowers/specs/2026-10-19-ios-kept-tabs-and-saldo-refresh-design.md, the maintainer approved]`
  Rejected. Android switches at once; parity was the ask.

## 4. Decision & Rationale for Corroboration

Option: kept tab slots in `TabSlots` under `ScaffoldView`'s body, as described in §2. Every screen with exactly one
selected tab renders inside `TabSlots` under one identity, so a tab switch happens inside it. Moving between a tabbed
and an untabbed screen animates as before. ADR-0032 holds: the slots are a render cache keyed by what the core sent.

**Mutation proof:** the invariant was broken six times on purpose in barbershop's `Render.swift`. The test failed each
time; the file was then restored with `git checkout -- <file>`, and the test passed again.
- `.allowsHitTesting(shown)` was removed (`ADR-0058: … a hidden slot must not be tapped or read`).
- The slot sent through `send(a)` instead of the gate (`… a hidden slot's send must be dropped`).
- `.video` was removed from `holdsLiveView` (`… holdsLiveView misses .video`).
- The `.secure` clause was removed from `keptWhenHidden` (`… keptWhenHidden misses .secure`).
- The `resignFirstResponder` call was removed (`… a switch must end editing`).
- `SlotCache` asked `holdsLiveView` instead of `keptWhenHidden`
  (`… SlotCache must ask keptWhenHidden before keeping a hidden screen`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** Android and iOS behave the same on a tab switch: the screen, its scroll position and its shell state
  survive, and the switch is instant.
- **Negative:** memory. Up to one extra screen per tab stays alive (live native views excluded).
- **Negative:** a hidden slot is still laid out and re-rendered on each core update, unlike Android's. With a few tabs
  this is small on iOS hardware; a slow update with many heavy tabs would show it first.
- **Negative:** tab switches no longer cross-fade on iOS.
- **Negative:** moving to an untabbed screen and back drops every kept slot, as on Android.
- **Negative:** the web shell still rebuilds a tab on every switch. Until it follows ADR-0057, it differs from the
  native shells.
