# Pinned bottom bar: a screen's main actions within thumb reach

**Request:** `docs/pinned-bottom-bar.md` (Moj Termin design set, #12, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as ui + core + mobiler-web
+ **barbershop** shells. Every demo shell, including `demos/fullstack-todo/mobile`, gets the new scaffold
field (the Swift `.scaffold(...)` pattern arity). Templates are ported in the release CLI PR.

## Problem

The booking screen keeps "Završi" / "Nedolazak" pinned at the bottom while the details scroll. A
scaffold has `fab` and `sheet` but nothing pinned, so the app puts the buttons at the end of the
content, and a long booking means scrolling to reach the most-used action.

## Decisions (approved 2026-09-29)

1. **ABI.**
   - `Widget::Scaffold` gains `bottom_bar: Option<Vec<Widget>>`, appended.
   - Every core builder that rebuilds a scaffold passes it through: `with_fab` / `set_fab`,
     `with_sheet`, `with_theme`, `with_refresh`, `with_labels`, `with_appearance` and any others.
   - This breaks full `Scaffold` literals and patterns, including Swift's positional pattern (14 → 15
     positions); a note marks it.
2. **Builder.**
   - `with_bottom_bar(widget, children)` sets it on a scaffold and does nothing elsewhere.
   - `None` or an empty `Vec` draws nothing: today's layout exactly.
3. **Layout.**
   - A row of `children`, equal widths (weight 1 each), a 12 gap, padding 16 horizontal and 12
     vertical.
   - Background: palette `surface_bar`, else the shell's bar colour. A 1-hairline top divider in
     `outline_variant`, else the shell divider.
   - Order from top to bottom: body, pinned bar, tabs (when shown at the bottom), system gesture
     inset.
   - The body ends above the bar; they don't overlap, so the last row is always reachable.
   - The FAB and the snackbar float above the bar. An open sheet covers it: native modal sheets do
     this, and on web the sheet's z-index is above the bar.
   - On web's wide layout (tabs as a left rail), the bar spans the body column at its bottom.
4. **Keyboard: the bar rises with the keyboard** on iOS (the keyboard safe area lifts the VStack)
   and on Android when there are no bottom tabs. With tabs, Android keeps it with the tab bar behind
   the keyboard, as tabs behave today (clarified after review).
   - iOS: bottom-inset content stays above the keyboard.
   - Android: an edge-to-edge window with IME padding on the bottom bar area.
   - Web: the bar is `position: sticky; bottom: 0` inside the scaffold, so it follows the visual
     viewport where the browser resizes it.

## Per shell

- **Web:**
  - After the body, before the tab bar: `<div class="bottombar">{row of children, each wrapped in .bottombar-cell}</div>`.
  - `.bottombar { position: sticky; bottom: <tab bar height when tabs are at the bottom, else 0>; display: flex; gap: 12px; padding: 12px 16px; background: var(--surface-bar, var(--bg)); border-top: 1px solid var(--line); z-index: 4 }`
    and `.bottombar-cell { flex: 1; min-width: 0 } .bottombar-cell > .btn { width: 100% }`.
  - The FAB's fixed `bottom` is lifted by the bar's height: a `has-bottombar` scaffold class plus
    `--bottombar-h`, set by the shell from the rendered bar's height.
  - The snackbar's placement also counts `.bottombar`.
- **Android (barbershop):**
  - The M3 `Scaffold`'s `bottomBar` slot becomes `Column { PinnedBar; NavigationBar }`.
    `PinnedBar` is a `Surface(color = bar)` holding a `Row(Modifier.fillMaxWidth().padding(16.dp, 12.dp), spacedBy(12.dp)) { children.forEach { Box(Modifier.weight(1f)) { Render(it) } } }`,
    with a top `HorizontalDivider`.
  - M3 lifts the FAB and snackbar above the `bottomBar` slot by itself.
  - Add `Modifier.imePadding()` on the pinned bar when there are no tabs; the tabs themselves hide
    behind the keyboard, as today.
- **iOS (barbershop):**
  - In `ScaffoldView`'s VStack, between the body group and the tab bar:
    `PaletteDivider(); HStack(spacing: 12) { ForEach(children) { render($0).frame(maxWidth: .infinity) } }.padding(.horizontal, 16).padding(.vertical, 12).background(bar)`.
  - The FAB and snackbar overlays sit on the body, which ends above the bar.
  - SwiftUI's keyboard safe area keeps the VStack's bottom content above the keyboard.
- **Other demo shells:** the Swift `.scaffold(...)` pattern gains `_`; Android needs nothing (fields
  by name). They don't draw the bar; the template gets barbershop's version at release.

## Demo

Barbershop: with a service selected (the Services tab's detail pane), the scaffold gets
`with_bottom_bar(root, vec![button("Book", Filled, Msg::OpenService(i)), button("Call", Tonal, Msg::CallShop)])`.

## Verification

- **Core unit tests:**
  - `with_bottom_bar` sets it and does nothing on non-scaffolds
  - `with_fab` / `with_sheet` / `with_theme` / `with_refresh` / `with_labels` / `with_appearance`
    keep a set bar
  - an empty `Vec` is stored
- **Web (CDP):**
  - on service detail, `.bottombar` bottom == `.tabbar` top, and it stays there after scrolling to
    the end
  - the last body element's bottom ≤ the bar's top
  - the FAB bottom ≤ the bar top − 12
  - the snackbar sits above the bar
  - opening the sheet covers it (the sheet's z-index is higher, and its rect is over the bar)
  - screens without a bar are unchanged; coffee/todo pixel-identical to `main`
- **Android (AVD, foreground checked before every action):**
  - the bar's bounds bottom == the NavigationBar top, and it doesn't move after a scroll
  - the FAB is above it
- **iOS:** CI compile only.

## Amendments

- **2026-10-02 (appointments request "pinned bar above keyboard with tabs"):** decision 4's Android exception is
  reversed. On a screen with bottom tabs, the pinned bar rides on the keyboard once the keyboard is taller than the
  tabs (which stay behind it): its extra bottom padding is the keyboard inset minus the tabs' measured height,
  applied at layout (`WindowInsets.ime.exclude(...)`), so it moves with the keyboard frame by frame. A send button
  under a message field was hidden behind the keyboard on every tabbed compose screen. (A first version hid the tabs
  while the keyboard was open; deciding that during composition lagged the keyboard by a frame and made the bar
  jump.) iOS is unchanged: the keyboard lifts the bar and the tabs together, so the bar sits one tab bar higher
  there (a known divergence; the Send button is visible either way). Android 11+ only: on older Android the window pans (no `adjustResize`), which the follow-up "the keyboard
  never covers the focused field" addresses.
- **2026-10-02 (follow-up "the keyboard never covers the focused field"):** the main activity now sets
  `windowSoftInputMode="adjustResize|stateHidden"`, so the keyboard inset reaches Compose on Android 8–10 too: the
  pinned bar rides on the keyboard there as well (emulator-checked on API 26). The scaffold body ends above the
  keyboard (`consumeWindowInsets(innerPadding).imePadding()`), so a focused field is scrolled into view.

