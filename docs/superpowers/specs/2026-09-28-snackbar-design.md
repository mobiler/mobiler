# Snackbar: feedback after an action, with an optional undo

**Request:** `docs/snackbar.md` (Moj Termin design set, #6, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as core + mobiler-web +
**barbershop** shells. Templates are ported in the release CLI PR.

## Problem

`cx.toast(text)` is fire-and-forget: an Android `Toast`, an iOS label, a web div. It has no action
and no duration choice, and on Android it floats over the bottom navigation and the FAB. The design
shows a snackbar after a state change ("Termin završen · Milan Jovanović"), with an **undo** action.

## Decisions (approved 2026-09-28)

1. **API.** A builder in `mobiler-core/src/dialog.rs`, next to `Confirm` / `Picker`:

   ```rust
   cx.snackbar(Snackbar::new("Booking cancelled").action("Undo"), |r| Msg::Undo(r.ok));
   Snackbar::new(text).action(label).long()   // default duration Short
   ```

   - `Short` ≈ 4 s, `Long` ≈ 10 s. Android may extend the timeout while accessibility services are on
     (M3 does this itself).
   - **Result:** `ok: true`, `output "action"` when the action is tapped. Otherwise `ok: false`, with
     `output` set to `"timeout"`, `"dismissed"` (swiped away) or `"replaced"` (a newer snackbar).
   - A snackbar without an action still takes `then`; it resolves `ok: false, "timeout"`.
   - With no scaffold on screen it resolves `ok: false, "timeout"` at once, without being shown.
   - iOS keeps it up at least 10 s while VoiceOver is running and it has an action.
2. **Wire.** A new built-in request/response capability `snackbar`, op `show`. The input is JSON
   `{"text", "action_label"?, "duration": "short" | "long"}`, like the `dialog` plugin.
   - No `mobiler-ui` change.
   - `cx.toast` stays as it is.
3. **One at a time.** Showing a snackbar while one is visible resolves the old one
   `ok: false, "replaced"` and shows the new one. This matches the one-confirm-at-a-time rule.
4. **Placement.** Above the bottom navigation **and** the FAB, never covering either. Horizontally
   inset from the screen edges.
5. **Colours: the M3 inverse convention, from existing roles** (no new palette tokens):
   - background = `on_surface`
   - text = `surface`
   - action = an inverse primary: `primary` mixed halfway toward the text colour. Plain `primary`
     on `on_surface` is about 2:1 (changed after review, 2026-09-28).
   - Without a palette, each shell's own default: Android M3 `SnackbarDefaults`, iOS/web a near-black
     surface with white text and the theme's primary action colour.
   - Corners: 4 dp/pt/px (the M3 snackbar default). No shape token.
6. **Accessibility.**
   - Android: M3 `Snackbar`'s own live region.
   - iOS: a VoiceOver announcement (`UIAccessibility.post(.announcement)`), with the action as a
     real `Button`.
   - Web: `role="status"` + `aria-live="polite"`, with the action as a `<button>`.

## Per shell

- **Android (barbershop).**
  - A `SnackbarPlugin` sets a `SnackbarHost`-style request object (like `ConfirmHost`).
  - `MainActivity`'s M3 `Scaffold` gets `snackbarHost = { SnackbarHost(state) }`. M3 already stacks
    the snackbar above the bottom bar and the FAB.
  - The request shows via `SnackbarHostState.showSnackbar(message, actionLabel, withDismissAction =
    false, duration)`. `SnackbarResult.ActionPerformed` → ok, `Dismissed` → timeout/dismissed.
  - Replacing: dismiss the current one and answer it `"replaced"` before showing the next.
  - `paletteScheme` fills `inverseSurface` / `inverseOnSurface` / `inversePrimary` from on_surface /
    surface / primary when those are set.
- **iOS (barbershop).**
  - `SnackbarPlugin` publishes to an `@Observable` host.
  - The root scaffold view overlays a capsule-free rounded rectangle (radius 4) at the bottom. It is
    placed above the tab bar and lifted above the FAB when a FAB is shown.
  - The action is a `Button`, and the view animates in and out. A timer resolves the timeout; a
    downward swipe dismisses.
- **Web (mobiler-web).**
  - A `.snackbar` element, separate from `.toast`, is mounted on `<body>` and carries the scaffold's
    theme variables (like the confirm modal).
  - `position: fixed`, with its bottom above the tab bar and above the FAB when one is present.
  - It resolves the timeout with a timer. The web has no swipe, so it only resolves as action,
    timeout or replaced.

## Demo

Barbershop's swipe-to-cancel on a booking row shows `Snackbar::new("Cancelled <booking>").action("Undo")`.
Undo puts the booking back at its old position.

## Verification

- **Core unit tests:**
  - the builder's JSON (with and without the action, short and long)
  - `cx.snackbar` emits a `snackbar`/`show` request
- **Web (CDP):**
  - the snackbar's rect is above the FAB's and the tab bar's
  - `role`/`aria-live` are present
  - tapping the action resolves `ok: true`: the booking comes back
  - letting it expire resolves `ok: false`, and the element is removed
  - a second snackbar replaces the first
  - coffee/todo pixel-identical to `main`
- **Android (AVD):**
  - uiautomator bounds: the snackbar's bottom is above the FAB's top and the nav's top
  - Undo restores the booking
  - a timeout leaves it cancelled
- **iOS:** CI compile only.

## Out of scope

- Queuing (we replace instead).
- A dismiss (×) button.
- Snackbar shape or colour tokens.
- Multi-line or two-action layouts.
- Migrating `toast`.
