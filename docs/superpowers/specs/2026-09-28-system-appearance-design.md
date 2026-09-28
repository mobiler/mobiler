# System appearance: Light / Dark / System, a query, and a change stream

**Request:** `docs/system-appearance.md` (Moj Termin design set, priority 2, blocking).
**Release:** part of the single Moj Termin design release (nothing published until the set is
done). This lands on `main` as core + mobiler-web + **barbershop** shells. Template shells and the
template `project.yml` are ported in the release's CLI PR (memory `template-shell-lands-after-publish`).
**Builds on:** `Theme.palette` (PR #221): the shell's resolved dark flag picks the palette set.

## Problem

The design offers Light, Dark and System. Today `Scaffold.dark_mode: bool` decides on every shell:
- iOS forces `.preferredColorScheme`.
- Android ignores the OS once a Scaffold is the root.
- Web has no `prefers-color-scheme` handling.

No capability reports the OS setting, and no event says it changed.

## Decisions

1. **`Scaffold.appearance: Option<Appearance>`**, set with a `with_appearance(root, a)` builder.
   - `None`: `dark_mode` decides, exactly as today.
   - `Light` / `Dark`: forced.
   - `System`: the shell follows the OS live, with no core round-trip.
2. **The shell resolves one `dark` flag:**
   `match appearance { Some(Light) => false, Some(Dark) => true, Some(System) => os_dark, None => dark_mode }`.
   Everything that used `dark_mode` now uses this flag: the palette set, the web `theme-dark`
   class, the Android M3 scheme, iOS `preferredColorScheme`, and the Android system bars.
3. **`cx.system_appearance(then)`:** a `device` op `appearance` that answers `"light"` or
   `"dark"`, the **OS** value, whatever the app forces.
4. **`cx.subscribe_appearance(key, on_event)`:** a new built-in `appearance` stream.
   - It emits the **current OS value immediately**, then one event per change.
   - `cx.unsubscribe(key)` stops it (full lifecycle in v1).
   - Events are `"light"` / `"dark"`.
5. **iOS minimum deployment target → 17.0** (user decision 2026-09-28: drop iPhone 8/8 Plus/X).
   - Demos move now.
   - The template `project.yml` moves in the release CLI PR, and `mobiler upgrade` 3-way merges
     it into apps.
   - iOS 17 allows `UIWindowScene.registerForTraitChanges` for a live OS value even while the
     app forces a scheme.

## ABI (mobiler-ui)

```rust
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Appearance { Light, Dark, System }

// Widget::Scaffold gains, as its LAST field:
    /// Light / Dark force a mode; System follows the OS live (the shell switches palette sets
    /// itself). `None` = `dark_mode` decides, as before.
    appearance: Option<Appearance>,
```

- Appending a field to the `Scaffold` variant changes its serialized shape. That is fine: shells
  are regenerated from the core they build against.
- Scaffold is built via builders (`scaffold`, `nav_scaffold`, `with_*`), which set
  `appearance: None`.
- Any Rust code that builds or destructures the full `Widget::Scaffold { … }` without `..` must
  be updated. That covers mobiler-ui tests and the mobiler-core builders. Demos use builders; the
  planner greps to confirm.

## Core (mobiler-core)

- `pub fn with_appearance(w: Widget, a: Appearance) -> Widget`, with the same pattern as
  `with_theme` / `with_labels`: it sets the field on a Scaffold root and is a no-op otherwise.
- `impl<E> Cx<E>`:
  - `pub fn system_appearance(&mut self, then: …)` sends `self.plugin("device", "appearance", "", then)`.
  - `pub fn subscribe_appearance(&mut self, key, on_event: impl Fn(PluginResponse) -> E …)` sends
    `self.subscribe(key, "appearance", "changes", "", on_event)`.
- Re-export `Appearance`.
- Tests:
  - builder sets and leaves the field
  - request/stream shape (plugin/op/input)
  - round-trip of a Scaffold with each appearance

## Shells

| | resolve `os_dark` for rendering | `device`/`appearance` op | `appearance` stream |
|---|---|---|---|
| **Web** | `matchMedia('(prefers-color-scheme: dark)').matches`, re-rendered on its `change` event (a signal the scaffold render reads) | same `matches` | `change` listener on the query; emits the current value first; the listener is removed on unsubscribe |
| **Android** | `isSystemInDarkTheme()` (recomposes on uiMode change: the Activity is recreated, and the ViewModel/core survives) | `resources.configuration.uiMode and UI_MODE_NIGHT_MASK` | `application.registerComponentCallbacks` (`onConfigurationChanged`) → flow; emits the current value first; unregistered on cancel. Works across Activity recreation |
| **iOS** | `System`: no `preferredColorScheme`, and `ScaffoldView` reads `@Environment(\.colorScheme)` to resolve the palette set; `Light`/`Dark`: forced as today | the key window scene's `traitCollection.userInterfaceStyle` (system-level; the SwiftUI override applies below the scene) | `AppearanceBridge` registers `registerForTraitChanges([UITraitUserInterfaceStyle.self])` on the active `UIWindowScene`; emits the current value first; unregistered on cancel |

- **Web detail:** the resolved dark flag feeds `theme_css(t, dark)`, the `theme-dark` class and
  the palette/page background, exactly where `dark_mode` feeds them today.
- **Android detail:** `val dark = when (appearance) { LIGHT -> false; DARK -> true; SYSTEM -> isSystemInDarkTheme(); null -> scaffold.darkMode }`.
  It replaces today's `darkMode ?: isSystemInDarkTheme()` for Scaffold roots.
- **iOS detail:**
  - `ActivePalette` is resolved from the resolved dark flag.
  - With `System`, `ScaffoldView` is the one place that knows the live scheme. So it sets
    `ActivePalette.current` and `.environment(\.paletteRoles, …)` from
    `@Environment(\.colorScheme)` on each body pass. Children render after, and the environment
    dependency re-renders palette readers.
  - Core.swift keeps resolving for `None`/`Light`/`Dark`.

## Demo

Barbershop's Home "Light theme" toggle becomes a segmented control: **Light · Dark · System**.
- Dark stays the default.
- The model holds an `Appearance`, and the view passes it with `with_appearance`.
- Barbershop also subscribes with `subscribe_appearance` and shows a caption "System: dark/light"
  that updates live. This exercises the stream on each shell.

## Verification

- **Core/ui unit tests** as listed.
- **Web** (headless Chrome, CDP `Emulation.setEmulatedMedia` with `prefers-color-scheme`):
  - `System` switches the palette set live, both directions.
  - `Light`/`Dark` stay put.
  - The caption (stream) updates.
  - No exceptions.
  - Coffee/todo still pixel-identical to a `main` build.
- **Android** (AVD, `adb shell cmd uimode night yes|no`):
  - `System` switches the palette (pixel samples) and the system-bar icons.
  - `Light`/`Dark` stay put.
  - The caption updates without restarting the app.
  - After an Activity recreation the stream keeps reporting.
- **iOS:** macOS CI compile of barbershop (with its target at 17.0). A runtime check waits for a
  simulator session.
- **Fresh whole-branch review**, as before.

## Out of scope

- Fonts, type scale, shapes, and the rest of the set.
- The `open_url` callback.
- Fixing the no-palette-path inconsistencies (still deliberately untouched).
