# Extended FAB: icon plus label

**Request:** `docs/extended-fab.md` (Moj Termin design set, #7, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as ui + core + mobiler-web
+ **barbershop** shells. Templates are ported in the release CLI PR.

## Problem

`Fab { icon, on_press }` is icon-only. The design's main action is "+ Novi termin", and part of the
user base isn't comfortable with icon-only controls.

## Decisions (approved 2026-09-28)

1. **ABI.** `Fab` gains `label: Option<String>`, appended last.
   - `None` = today's round FAB, rendered byte-identically on every shell.
   - This breaks `Fab { .. }` struct literals (a comment marks it). Apps build FABs through the
     builders, so in practice nobody is affected.
2. **Builder.** `with_extended_fab(widget, icon, label, on_press)` sets the label. `with_fab` stays
   exactly as it is, with `label: None`.
3. **Look.** Same anchor, above the bottom nav at the trailing edge, and same height, 56.
   - Icon, then the label, with 16 horizontal padding and a 12 gap (M3 extended FAB).
   - Same colours: palette `fab` / `on_fab`, else each shell's default.
   - Same `Theme.shapes.fab` radius, else the shell's default FAB shape.
4. **Accessibility.** The label is the accessible name; the icon is decorative.
5. **Out of scope:** collapsing to icon-only on scroll, and a label without an icon.

## Per shell

- **Web:**
  - `<button class="fab fab-extended">` holding the icon glyph (`aria-hidden`) and
    `<span class="fab-label">`.
  - `.fab-extended { width: auto; padding: 0 16px 0 14px; gap: 12px; font-size: 16px; font-weight: 600 }`,
    where the glyph keeps its size.
- **Android (barbershop):** `ExtendedFloatingActionButton(text = { Text(label) }, icon = { Icon(…, null) },
  onClick, shape = shapeOf { it.fab } ?: FloatingActionButtonDefaults.extendedFabShape)`, with the same
  colour handling as the round FAB.
- **iOS (barbershop):** the FAB button's content becomes `HStack(spacing: 12) { Image; Text(label) }`,
  with `.padding(.horizontal, 16)` and `.frame(height: 56)`. It keeps the same background,
  foreground, shape and shadow. `.accessibilityLabel(label)`.

## Demo

Barbershop's Home FAB becomes `with_extended_fab(…, Icon::Calendar, "Book", Msg::Book)`.

## Verification

- **ui/core:**
  - a round-trip of a labelled `Fab`
  - `with_extended_fab` sets the label
  - `with_fab` leaves it `None`
- **Web (CDP):**
  - barbershop's `.fab-extended` shows "Book"
  - its accessible name is "Book"
  - its bottom is above the tab bar's top
  - its height is 56
  - a snackbar still sits 12px above it
- **Web pixels:** coffee, todo and saldo (round FAB) pixel-identical to `main`.
- **Android (AVD):** the label node "Book" sits inside the FAB's bounds, above the nav. Check the
  foreground app before every tap.
- **iOS:** CI compile only.
