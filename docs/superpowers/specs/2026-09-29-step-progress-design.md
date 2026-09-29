# Step indicator for multi-step flows

**Request:** `docs/step-progress.md` (Moj Termin design set, #10, polish).
**Release:** part of the single Moj Termin design release. It lands on `main` as ui + core + mobiler-web
+ **barbershop** shells. Every demo shell, including `demos/fullstack-todo/mobile`, gets the new widget
case. Templates are ported in the release CLI PR.

## Problem

A three-step flow shows "Korak 2 od 3" over a bar of three segments with two filled. `progress(Some(f))`
is one continuous bar: 2/3 of a bar doesn't read as "step 2 of 3", and screen readers announce a
percentage.

## Decisions (approved 2026-09-29)

1. **ABI.**
   - `Widget::Steps { total: u8, current: u8 }`, appended. It's a new case in every shell's
     exhaustive widget switch.
   - Builder `steps(total, current)`, e.g. `steps(3, 2)`. The builder clamps `current` to `total`.
2. **Look.**
   - `total` equal segments across the available width, with a 4 dp/pt/px gap between them.
   - Each segment is 4 tall and fully rounded.
   - The first `current` are filled in the palette's `primary`, else the brand colour; the rest in
     `surface_muted`, else the shell's muted surface.
   - `total == 0` renders nothing. The shells clamp `current` too, for hand-built widgets.
   - Any "Korak 2 od 3" text is the app's own `text(..)`.
3. **Accessibility.** The indicator is one element whose spoken text is the step string; no
   percentage anywhere.
   - Web: `role="progressbar"`, `aria-valuemin=1`, `aria-valuemax=total`, `aria-valuenow=current`
     and `aria-valuetext` set to the string.
   - Android: `semantics { contentDescription = text }` on the row, with no `progressBarRangeInfo`.
   - iOS: `.accessibilityElement(children: .ignore).accessibilityLabel(text)`.
4. **Language.**
   - `ShellLabels.step_of: Option<String>`, appended, with the builder `.step_of(template)`. The
     template uses `{current}` and `{total}`, e.g. `"Korak {current} od {total}"`.
   - The default is `"Step {current} of {total}"`, and an empty template also falls back to it, like
     the other labels.
   - Each shell reads it from the root scaffold's labels, as it does for the other `ShellLabels`.

## Per shell

- **Web:**
  - `<div class="steps" role="progressbar" …>` holding `total` `<span class="step">` elements; the
    filled ones get `step-on`.
  - CSS: `.steps { display: flex; gap: 4px; width: 100%; } .step { flex: 1; height: 4px; border-radius: 2px; background: var(--surface-2); } .step-on { background: var(--primary); }`
- **Android (barbershop):**
  - `Row(Modifier.fillMaxWidth().semantics { contentDescription = text }, spacedBy(4.dp)) { repeat(total) { i -> Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(50)).background(if (i < current) primary else surfaceVariant)) } }`
  - Colours: the palette `primary` else `MaterialTheme.colorScheme.primary`, and `surfaceVariant`,
    which `paletteScheme` fills from `surface_muted`.
- **iOS (barbershop):**
  - `HStack(spacing: 4) { ForEach(0..<total) { Capsule().fill(i < current ? primary : muted).frame(height: 4) } }`,
    with the accessibility modifiers.
  - Colours: `role(pal?.primary, else: brand)` and `role(pal?.surfaceMuted, else: Color(.systemFill))`.
- **Other demo shells:** the same rendering, minus the palette. The web implementation is shared; the
  native demos get a simple version (primary / grey), so the case compiles and works everywhere.

## Demo

Barbershop's booking sheet shows `steps(3, 2)` under its title, with a `caption("Step 2 of 3")` above
it.

## Verification

- **Core unit tests:**
  - `steps` builds and clamps
  - the `ShellLabels` builder sets `step_of`
  - a Rust helper `step_text(template: Option<&str>, current, total)`, used by web, substitutes the
    placeholders and falls back to English
- **Web (CDP):**
  - three `.step` elements, two `step-on`, equal widths
  - `aria-valuetext` "Step 2 of 3"
  - no exceptions
  - coffee/todo pixel-identical to `main`
- **Android (AVD, foreground app checked before every action):** a node with the content-desc
  "Step 2 of 3"; a screenshot shows 3 segments, 2 filled.
- **iOS:** CI compile only.
