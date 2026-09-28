# Badge icons and avatar initials

**Request:** `docs/badge-icon-avatar-initials.md` (Moj Termin design set, #8, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as ui + core + mobiler-web
+ **barbershop** shells. Every demo shell gets the new enum case and patterns. Templates are ported in
the release CLI PR.

## Problem

A status is never shown by colour alone: each status chip is an icon plus text. `Badge { label, tone }`
has no icon. Clients have no photos, so avatars show initials, but `Avatar { source, status }` takes an
image only, and an empty source draws an empty circle.

## Decisions (approved 2026-09-28)

1. **ABI.** Fields appended; `None` = today's rendering.
   - `Widget::Badge { label, tone, icon: Option<Icon> }`
   - `Widget::Avatar { source, status, initials: Option<String>, size: Option<u8> }`
   - This breaks exhaustive `Widget::Badge { .. }` / `Widget::Avatar { .. }` patterns and full
     literals, and iOS positional `.badge(...)` / `.avatar(...)` patterns. A field note marks it.
2. **New icon:** `Icon::DoneAll`, a double check for "finished", appended last.
   - Android `Icons.Filled.DoneAll`, web "✓✓", iOS `checkmark.circle` (SF Symbols has no double
     check).
   - Every demo shell's icon mapper gets the case.
3. **Builders.** `badge` / `avatar` / `avatar_status` are unchanged. New set-field-or-no-op modifiers:
   - `with_icon(widget, icon)` sets a `Badge`'s icon.
   - `with_initials(widget, "MJ")` and `with_avatar_size(widget, 40)` set an `Avatar`'s fields.
   - Each does nothing on any other widget.
4. **Badge icon.**
   - Before the label, sized to the label's line height: 14 dp/pt/px, with a 4 gap.
   - In the tone's foreground colour.
   - Decorative: `contentDescription = null` / `accessibilityHidden` / `aria-hidden`. Only the label
     is read.
5. **Initials.**
   - When they're drawn: `source` is empty, or the image fails to load. A loading or loaded image wins.
   - The circle: palette `secondary_container` background with `on_secondary_container` text.
     Without a palette, a 16% tint of the brand colour with brand-coloured text: web `--accent-soft` /
     `--primary-text` else `--primary`, Android M3 `secondaryContainer` / `onSecondaryContainer`.
   - Text is at most 2 characters (the shell takes the first two), 40% of the diameter, semibold, in
     the body font.
   - Accessibility: no own name, since the app shows the name next to it.
6. **Size.** `None` = 48. The size applies to the image or the initials circle. The status dot stays
   12, at the bottom-end corner.

## Per shell

- **Web:**
  - Badge: `<span class="badge …"><span class="badge-icon" aria-hidden="true">glyph</span>label</span>`,
    and today's markup without an icon.
  - Avatar: when `initials` is set, `<span class="avatar-initials">` sits under the `<img>`. The img
    is omitted when `source` is empty, and hidden `onerror`.
  - Size: an inline `width` / `height` on `.avatar` and its children, only when set.
- **Android (barbershop):**
  - Badge: a `Row` holding `Icon(14.dp, tint = fg)` and the `Text`.
  - Avatar: `SubcomposeAsyncImage`, or `AsyncImage` with an error/empty fallback, drawing the
    initials `Box`. Size from `widget.size`, else 48.
- **iOS (barbershop):**
  - Badge: an `HStack(spacing: 4)` with `Image(systemName:)` (`.font(.system(size: 14))`,
    `accessibilityHidden`) and the `Text`.
  - `AvatarView(source:status:initials:size:)`: the `AsyncImage` phase `.failure`, or an empty
    source, draws the initials circle.
- **Other demo shells (coffee, todo, saldo):** the new `DoneAll` mapper case; the iOS `.badge` /
  `.avatar` patterns ignore the new fields (`_`).

## Demo

Barbershop gets:
- A booking-status row on Bookings: `with_icon(badge("Confirmed", Success), Check)`,
  `with_icon(badge("Pending", Warning), Clock)`, `with_icon(badge("Finished", Info), DoneAll)` and
  `with_icon(badge("No-show", Danger), Close)`.
- A client card with `with_avatar_size(with_initials(avatar(""), "MJ"), 40)`.

## Verification

- **Core unit tests:** each `with_*` sets its field and does nothing elsewhere; round-trips.
- **Web (CDP):**
  - `.badge-icon` precedes the label and has `aria-hidden`
  - an empty source → initials "MJ", 40×40
  - a broken URL → initials shown
  - no exceptions
  - coffee/todo pixel-identical to `main`
- **Android (AVD, foreground app checked first):** the badge text nodes; the "MJ" node inside a
  40dp (105px at 2.625) circle.
- **iOS:** CI compile only.
