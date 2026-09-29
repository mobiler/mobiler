# Grid columns and a dashed card style

**Request:** `docs/grid-columns-and-dashed-cards.md` (Moj Termin design set, #9, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as ui + core + mobiler-web
+ **barbershop** shells. Every demo shell, **including `demos/fullstack-todo/mobile`**, gets the new
card case and the iOS grid pattern arity. Templates are ported in the release CLI PR.

## Problem

The booking flow shows time slots 3 per row and providers 2 per row. `Grid { children }` picks its own
column count (2 on a phone, more on a tablet). A free gap in the day view is a dashed-outline row
("empty, you can fill this"), and `CardStyle` has no dashed style.

## Decisions (approved 2026-09-29)

1. **ABI.**
   - `Widget::Grid { children, columns: Option<u8> }`, appended.
   - `CardStyle::Dashed`, appended.
   - This breaks full `Grid` literals and patterns (including iOS positional `.grid(...)`) and
     exhaustive `CardStyle` matches. A note on the field and variant marks it.
2. **Columns.**
   - `None` = today's adaptive grid, unchanged: web `1fr 1fr`, Android `max(2, width / 190dp)`,
     iOS `GridView`'s compact/regular rule.
   - `Some(n)` = exactly `n` equal columns at every width, clamped to 1–4 in the shells. Children
     wrap into rows; a short last row keeps the empty cells, so the widths stay equal.
   - The gap stays 12.
3. **Builder.** `with_columns(widget, n)` sets a `Grid`'s columns, clamped to 1–4, and does nothing
   on any other widget. `grid` is unchanged.
4. **Dashed card.**
   - No fill, and a 1.5 dp/pt/px dashed stroke (dash 6, gap 4) in the palette's `outline`, else the
     shell's outline colour (web `var(--outline, var(--line))`).
   - The card's radius: `Theme.shapes.card`, else the shell's card radius.
   - Content colour as on the outlined card.
   - Pressable and long-pressable like every card style (`card_button`, `with_long_press`).

## Per shell

- **Web:**
  - Grid: when `columns` is set, an inline `grid-template-columns: repeat(n, minmax(0, 1fr))` on
    `.grid`; otherwise today's markup.
  - `.card-dashed { background: transparent; border: 1.5px dashed var(--outline, var(--line)); }`
- **Android (barbershop):**
  - Grid: `cols = widget.columns?.toInt()?.coerceIn(1, 4) ?: <today's adaptive rule>`, with the
    same chunked `Row`s and weights.
  - Dashed: a `Box` (like Brand) with `fillMaxWidth().clip(shape).drawBehind { drawOutline(shape.createOutline(size, layoutDirection, this), color, style = Stroke(1.5.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx())))) }.then(clickMod)`
    around `CardBody`.
  - Every demo's exhaustive `when (widget.style)` gets `CardStyle.DASHED`. The barbershop version
    draws it; the other demos draw it as `OUTLINED` (their shells aren't the release target).
- **iOS (barbershop):**
  - `case .grid(let children, let columns)` → `GridView(children:columns:send:)`, which uses
    `columns` when set.
  - `CardMod`: `case .dashed: content.overlay(shape.stroke(role(pal?.outline, else: Color.gray.opacity(0.5)), style: StrokeStyle(lineWidth: 1.5, dash: [6, 4])))`.
  - Other demos: `.grid(let children, _)` and a `.dashed` case drawn like `.outlined`.

## Demo

Barbershop Bookings gains:
- A "Free times" grid: `with_columns(grid(tiles), 3)`, where the tiles are six small outlined
  cards "09:00" … "11:30".
- A dashed free-slot card: `card_button(text("Free 12:45 – 13:30 · 45 min · + Book"), CardStyle::Dashed, Msg::Book)`.

## Verification

- **Core unit tests:** `with_columns` sets and clamps (0 → 1, 9 → 4), does nothing elsewhere, and a
  `Dashed` round-trip.
- **Web (CDP):**
  - the "Free times" grid has 3 equal column widths
  - the existing `None` grids still compute 2 columns
  - the dashed card computes `border-style: dashed`, a 1.5px width and a transparent background
  - clicking it opens booking (the sheet appears)
  - no exceptions
  - coffee/todo pixel-identical to `main`
- **Android (AVD, foreground app checked before every action):**
  - the three tiles in a row have equal widths
  - tapping the dashed card opens the booking flow
  - a screenshot shows the dashes
- **iOS:** CI compile only.
