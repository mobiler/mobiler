# Component shapes: a corner radius per component

**Request:** `docs/component-shapes.md` (Moj Termin design set, #5, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as core + mobiler-web +
**barbershop** shells. Templates are ported in the release CLI PR.

## Problem

`Theme.corner` is one global step (`None/Small/Medium/Large`), and some shapes are fixed in the
shells (for example the badge pill). The design needs its own radius per component:
- card 12
- buttons pill
- FAB 16
- sheet top corners 28
- chip 8
- badge fully round

## Decisions (approved 2026-09-28)

1. **`Theme.shapes: Option<Shapes>`**, with `Shapes { card, button, fab, sheet_top, chip, badge, input: Option<Radius> }`
   and `Radius { Dp(u8), Pill }`.
   - `None`, or a component left `None`, keeps today's shape, derived from `corner` or fixed by the
     shell.
   - This is another `Theme` field, so Theme literals need `..Default::default()`: the same accepted
     break as `palette`/`type_scale`. Barbershop's literal gets the field.
2. **Where each radius applies:**

   | Radius | Applies to |
   |---|---|
   | `card` | every `CardStyle` (elevated, outlined, filled, brand), including tappable cards |
   | `button` | every `ButtonStyle` (filled, outlined, text, tonal), toned buttons and the `Density::Large` size |
   | `fab` | the scaffold FAB |
   | `sheet_top` | the bottom sheet's top two corners; its bottom corners are unchanged |
   | `chip` | `Chip` |
   | `badge` | `Badge` |
   | `input` | `TextField` (all kinds) and `SearchField` |

3. **`Pill` is fully round, half the component's height:**
   - Android: `RoundedCornerShape(percent = 50)`
   - iOS: `Capsule()` (or `RoundedRectangle(cornerRadius: .infinity)` where a shape type is needed)
   - web: `border-radius: 999px`

   It stays a pill at any density (acceptance 2).
4. **`Dp(n)`:** Android `n.dp`, iOS `n` pt, web `n` px.
5. **Out of scope:** segmented control, image corners (`ImageShape` keeps its own), progress bars,
   the calendar day cells, and the web confirm dialog card.

## Per shell

- **Web:**
  - `theme_css` emits `--r-<component>:<n>px|999px;` for each set radius.
  - The CSS rules for `.card`, `.btn`, `.fab`, `.sheet` (top corners), `.chip`, `.badge`,
    `.field` and `.searchfield` read `var(--r-<component>, <today's value>)`.
  - A theme without `shapes` gives byte-identical output.
- **Android (barbershop):** a `shapeFor(radius, default)` helper. The call sites pass `shape = …`
  only when the radius is set, so without shapes the calls are unchanged:
  - Card/OutlinedCard, the Brand card clip, the tappable card
  - `Button`/`OutlinedButton`/`TextButton`/`FilledTonalButton`
  - `FloatingActionButton`
  - `ModalBottomSheet(shape = RoundedCornerShape(topStart, topEnd))`
  - `FilterChip`, the badge `Box` clip
  - `OutlinedTextField`
- **iOS (barbershop):**
  - `CardMod` shape, `PaletteButtonStyle`/`LargeButtonStyle`/`TonalButtonStyle` and the
    `.borderedProminent` family.
  - For bordered buttons with a set radius, use `.buttonBorderShape(.capsule)` or
    `.roundedRectangle(radius:)` (iOS 17).
  - FAB clip, the sheet panel (top-corner shape via `UnevenRoundedRectangle`, iOS 17), chip
    capsule, badge capsule, and the text-field background (`PaletteFieldStyle`) and search capsule.
  - Without shapes, today's expressions are unchanged.

## Demo

Barbershop sets the design's shapes: card `Dp(12)`, button `Pill`, fab `Dp(16)`, sheet_top `Dp(28)`,
chip `Dp(8)`, badge `Pill`, input `Dp(12)`.

## Verification

- **ui/core:** a round-trip of a `Theme` with `shapes`, and `Radius` variants.
- **web:**
  - unit tests: `theme_css` emits only the set radii; no shapes gives identical output.
  - CDP: computed `border-radius` of `.card` = 12px, `.btn` = 999px (a pill), `.fab` = 16px,
    `.sheet` top-left/top-right = 28px with its bottom unchanged, `.chip` = 8px, `.badge` = 999px,
    `.field` = 12px.
  - Coffee/todo pixel-identical to `main`.
- **Android:**
  - A barbershop screenshot vs `main`: cards, buttons, FAB, chips and badges visibly take the new
    radii.
  - Sample a corner pixel on a card: the background shows through at the 12dp corner.
  - `Density::Large` buttons stay pills.
- **iOS:** CI compile only.
