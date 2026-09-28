# Badge Icons and Avatar Initials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** status badges can carry an icon, and avatars draw initials (at an optional size) when there's
no image. Existing badges and avatars render as today.

**Architecture:**
- ABI: fields appended to `Widget::Badge` / `Widget::Avatar`, plus `Icon::DoneAll`.
- Core: `with_icon`, `with_initials` and `with_avatar_size` modifiers.
- Shells: web + barbershop Android/iOS render the new fields; the other demo shells get only the
  mapper case and the pattern arity.

**Tech Stack:** Rust (ui/core/web), Compose M3 + Coil 3, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-28-badge-icon-avatar-initials-design.md`

## Global Constraints

- **Unchanged when unset.** `icon: None`, `initials: None` and `size: None` keep today's
  markup/composables.
- **Badge icon:** 14 dp/pt/px, a 4 gap, the tone foreground colour, decorative.
- **Initials:**
  - at most 2 characters (the first two)
  - 40% of the diameter, semibold, in the body font
  - palette `secondary_container` / `on_secondary_container`, else a 16% brand tint + brand text
  - drawn when the source is empty or the image failed; a loading or loaded image wins
- **Sizes:** default 48; the status dot stays 12.
- **Kotlin enum name:** `WidgetIcon.DONEALL`; Swift `.doneAll`.
- **Repo rules:**
  - don't touch the templates
  - don't publish
  - Swift is gated in CI
  - per-demo `CARGO_TARGET_DIR`
  - commit bodies via `git commit -F -` with a quoted heredoc
  - **the AVD is shared:** check that `mCurrentFocus` is `dev.mobiler.barbershop` before any tap
    or dump

## Review Focus

1. **Image failure.** A broken URL shows the initials on every shell, not a grey circle. Pinned by
   Task 5 (web CDP, broken URL); native is judged by review.
2. **Emoji or multi-codepoint initials.** Take the first two *characters*: Rust `chars().take(2)`,
   Kotlin `take(2)`, Swift `prefix(2)`. No byte slicing.
3. **Other demos compile.** coffee, todo and saldo get the Android mapper case and the iOS patterns.
   The Android side is pinned by a local APK build of each (Task 3); iOS by CI.
4. **Size with status.** The dot stays at the bottom-end corner of the resized circle.

---

### Task 1: ABI + builders

**Files:** `mobiler-ui/src/lib.rs`, `mobiler-core/src/lib.rs`, and any Rust match on
`Widget::Badge` / `Widget::Avatar` (mobiler-web's render; compile errors list them).

- [ ] **Step 1: Tests** (mobiler-core):

```rust
#[test]
fn badge_icon_and_avatar_initials_modifiers() {
    assert!(matches!(with_icon(badge("ok", Tone::Success), Icon::Check), Widget::Badge { icon: Some(Icon::Check), .. }));
    assert!(matches!(badge("ok", Tone::Success), Widget::Badge { icon: None, .. }));
    let a = with_avatar_size(with_initials(avatar(""), "MJ"), 40);
    assert!(matches!(&a, Widget::Avatar { initials: Some(i), size: Some(40), .. } if i == "MJ"));
    assert!(matches!(avatar("u"), Widget::Avatar { initials: None, size: None, .. }));
    // No-ops elsewhere.
    assert!(matches!(with_icon(text("x"), Icon::Check), Widget::Text { .. }));
    assert!(matches!(with_initials(badge("b", Tone::Info), "MJ"), Widget::Badge { .. }));
}
```

  In mobiler-ui, add a round-trip of a `Widget::Badge` with an icon and a `Widget::Avatar` with
  initials and a size, plus `Icon::DoneAll`.
- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.**
  - The fields, with doc comments and a BREAKING note on each.
  - `Icon::DoneAll` appended after `Scissors` (comment: double check, "finished").
  - The builders set the fields to `None`.
  - Add `with_icon`, `with_initials` and `with_avatar_size` (`#[must_use]`, set-field-or-passthrough,
    like `with_long_press`) and re-export them where the other `with_*` are.
  - mobiler-web: add `DoneAll => "✓✓"` in `icon_glyph`, and update the Badge/Avatar match arms with
    `..` for now (rendering comes in Task 2).
- [ ] **Step 4: GREEN.** Run the ui/core/web tests, clippy, the wasm check, and `cargo check` on
  every demo.
- [ ] **Step 5: Commit:** `feat(ui)!: badge icon, avatar initials/size, Icon::DoneAll`

### Task 2: Web

**Files:** `mobiler-web/src/lib.rs`, `mobiler-web/src/mobiler.css`.

- [ ] **Step 1: Test** (host-testable helper):

```rust
#[test]
fn initials_take_two_characters() {
    assert_eq!(initials_text("MŽX"), "MŽ"); // a cap, not name parsing: the app passes initials
    assert_eq!(initials_text("MJ"), "MJ");
    assert_eq!(initials_text("Ž"), "Ž");
    assert_eq!(initials_text("👩‍🔧x"), "👩\u{200d}");
}
```

  (`chars().take(2)`. The last case documents that grapheme clusters are not handled; that's
  accepted.)
- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.**
  - **Badge.** With `Some(icon)`: `<span class=class><span class="badge-icon" aria-hidden="true">{glyph}</span>{label}</span>`.
    With `None`: today's markup.
  - **Avatar.** With `initials: None` and `size: None`: today's markup exactly. Otherwise:
    - An outer `<span class="avatar" style=size>`, where `size` is
      `width:{n}px;height:{n}px;font-size:{0.4n}px` and only emitted when size is set.
    - Then, if initials are set, `<span class="avatar-initials">{initials_text}</span>`.
    - Then, if the source is non-empty, `<img class="avatar-img" src=… onerror="this.style.display='none'">`.
      The img sits above the initials, so a loaded image covers them. Use a Leptos `on:error`
      handler that sets `display:none` if inline attribute handlers are unavailable.
    - Then the dot.
  - **CSS:**

```css
.badge-icon { display: inline-block; font-size: 14px; line-height: 1; margin-inline-end: 4px; }
.avatar-img { position: relative; }
.avatar-initials { position: absolute; inset: 0; border-radius: 50%; display: flex; align-items: center;
  justify-content: center; background: var(--accent-soft); color: var(--on-secondary-container, var(--primary-text, var(--primary)));
  font-family: var(--font); font-weight: 600; font-size: inherit; }
.avatar:has(.avatar-initials) { font-size: 19.2px; } /* 40% of 48 when no size is set */
.avatar[style] .avatar-img { width: 100%; height: 100%; }
```

    `--accent-soft` is already `secondary_container` under a palette, else the 16% seed tint.
    Check that `.avatar-img`'s existing 48px size rule doesn't fight the resized container.
- [ ] **Step 4:** Run the tests and wasm clippy. Commit: `feat(web): badge icon + avatar initials/size`

### Task 3: Android (barbershop + other demos' mapper)

- [ ] **Barbershop** `MainActivity.kt`:
  - **Badge:** the Box content becomes
    `Row(verticalAlignment = CenterVertically, horizontalArrangement = spacedBy(4.dp)) { widget.icon?.let { Icon(iconFor(it), null, Modifier.size(14.dp), tint = fg) }; Text(...) }`.
    Keep the unchanged path's exact `Text` when the icon is null (a Row with one Text looks the same;
    acceptable).
  - **Avatar:**
    - `val d = (widget.size?.toInt() ?: 48).dp`.
    - The initials composable: `Box(Modifier.size(d).clip(CircleShape).background(MaterialTheme.colorScheme.secondaryContainer), contentAlignment = Center) { Text(initials.take(2), color = onSecondaryContainer, fontWeight = SemiBold, fontSize = (d.value * 0.4f).sp, fontFamily = <body family used by Text defaults>) }`.
    - When `source` is empty and initials are set: only the initials.
    - Otherwise use `SubcomposeAsyncImage(model, null, contentScale = Crop, modifier = Modifier.size(d).clip(CircleShape), error = { initials?.let { Initials() } })`,
      from `coil3.compose.SubcomposeAsyncImage`. Check the Coil 3 API names.
    - With no initials, keep today's `AsyncImage` call at 48 or the given size.
    - The status dot as today.
  - `iconFor`: add `WidgetIcon.DONE_ALL -> Icons.Default.DoneAll`.
- [ ] **coffee / todo / saldo** `MainActivity.kt` `iconFor`: add the same line.
- [ ] **Build** each Android APK: barbershop and saldo with `mobiler build android`; coffee and todo
  too, since each has a MainActivity mapper. Commit:
  `feat(android): badge icon + avatar initials (barbershop); DoneAll in every demo`

### Task 4: iOS (barbershop + other demos' patterns)

- [ ] **Barbershop** `Render.swift`:
  - **Badge.** `case .badge(let label, let tone, let icon):` with
    `HStack(spacing: 4) { if let icon { Image(systemName: sfSymbol(icon)).font(.system(size: 14)).accessibilityHidden(true) }; Text(label).font(.footnote.weight(.semibold)) }`,
    then the same padding, background, foreground and clip.
  - **Avatar.** `case .avatar(let source, let status, let initials, let size):` → `AvatarView(source:status:initials:size:)`.
    - `let d = CGFloat(size ?? 48)`.
    - An `initialsView`: `Circle().fill(role(pal?.secondaryContainer, else: brand.opacity(0.16)))` with
      `.overlay(Text(String((initials ?? "").prefix(2))).font(CustomFonts.bodyOr(.system(size: d * 0.4, weight: .semibold), size: d * 0.4, relativeTo: .body).weight(.semibold)).foregroundColor(role(pal?.onSecondaryContainer, else: brand)))`.
    - When the source is empty and initials are set: `initialsView`.
    - For file URLs: as today.
    - Otherwise `AsyncImage(url:) { phase in switch phase { case .success(let img): img.resizable().aspectRatio(contentMode: .fill); case .failure: initials != nil ? initialsView : grey; default: grey } }`.
    - Frame `d`, then the dot as today.
    - `brand` = `ActiveTheme.current?.brandColor ?? .accentColor`; check the accessor name.
  - `sfSymbol`: `case .doneAll: return "checkmark.circle"`.
- [ ] **coffee / todo / saldo** `Render.swift`: `case .badge(let label, let tone, _):`,
  `case .avatar(let source, let status, _, _):`, and `case .doneAll: return "checkmark.circle"` in
  their `sfSymbol`.
- [ ] **Commit:** `feat(ios): badge icon + avatar initials (barbershop); new patterns in every demo`

### Task 5: Barbershop demo + acceptance

- [ ] **Step 1: Test first:** the Bookings screen view contains four badges with icons (walk the
  widget tree, or assert on the builder helper the screen uses), and a 40-size avatar with "MJ".
  Write it against whatever helper the file already uses to find widgets in views. RED.
- [ ] **Step 2:** Add a "Status" row on Bookings with Confirmed/Check/Success, Pending/Clock/Warning,
  Finished/DoneAll/Info and No-show/Close/Danger, plus a client row with
  `with_avatar_size(with_initials(avatar(""), "MJ"), 40)` next to `text("Milan Jovanović")`. GREEN,
  then clippy.
- [ ] **Step 3: Web CDP:**
  - the first `.badge-icon` exists before its text node and has `aria-hidden`
  - `.avatar-initials` text is "MJ" and `.avatar` is 40×40
  - broken image: evaluate a DOM check by setting the avatar img `src` to a 404 URL; the initials
    become visible (`img` `display:none`)
  - no exceptions
  - coffee/todo pixel-identical to `main`
- [ ] **Step 4: Android AVD** (check the foreground app before every action): the dump has
  "Confirmed", "Pending", "Finished" and "No-show", and an "MJ" node whose parent bounds are about
  105px square. Take a screenshot.
- [ ] **Step 5: Commit:** `feat(barbershop): status badges with icons + initials avatar`

### Task 6: Docs, review, PR

- [ ] Add a NOTES.md section, then run a fresh whole-branch review (opus) and a fix pass.
- [ ] Ship with ship-pr (25 checks) and squash-merge.
- [ ] Update the memory and `start.md`.
