# System Appearance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apps choose Light / Dark / System (`with_appearance`). Under System each shell follows the
OS live and switches the palette set itself. Apps can query the OS value (`cx.system_appearance`)
and subscribe to its changes (`cx.subscribe_appearance`). The iOS minimum becomes 17.

**Architecture:**
- The ABI gets `Appearance` and a last `Scaffold.appearance` field.
- Every shell resolves one `dark` flag from `appearance` and `os_dark`, and it replaces
  `dark_mode` everywhere it was read.
- A new built-in `appearance` stream (op `changes`) emits the current OS value, then each change.
- A new `device` op `appearance` answers once.
- Native work goes into **barbershop's** shells. The Swift `.scaffold(…)` positional patterns
  in **every** demo gain one `_`. Templates are not touched (they are ported at the release).

**Tech Stack:** Rust/Crux, Leptos/WASM, Kotlin/Compose, SwiftUI/UIKit (iOS 17).

**Spec:** `docs/superpowers/specs/2026-09-28-system-appearance-design.md`

## Global Constraints

- **`appearance: None` must render exactly as today on every shell.** The dark flag is
  `dark_mode`, and on Android a non-Scaffold root still uses `isSystemInDarkTheme()`.
- Events and op answers are exactly `"light"` / `"dark"`.
- The stream emits the current value first. Unsubscribe removes the native listener: no leak,
  no emits after cancel.
- Query and stream report the **OS** value even while the app forces Light/Dark.
- Do not edit `mobiler/templates/`. Do not bump versions or publish.
- **Build rules:**
  - Per-demo `CARGO_TARGET_DIR=$PWD/target`, and
    `JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk`.
  - The CLI is `/media/zmilan/data2/cargo-target/debug/mobiler`.
  - Long builds run in the background; no `pgrep`/marker polling.
  - A web baseline comes from a `git worktree` of `main` with its own `CARGO_TARGET_DIR`, removed
    afterwards.
- **Swift:** a static touched from closures uses `nonisolated(unsafe)`. Only CI compiles Swift.

## Review Focus

1. **`appearance: None` path.** Unchanged on every shell. Pinned by the Task 5 coffee/todo web
   pixel diff vs a `main` build, and the Android coffee screenshot diff.
2. **System switches the palette set live** in both directions, including web `<html>` `--bg`
   and Android bars. Pinned by the Task 5 web CDP `setEmulatedMedia` and Android `cmd uimode`
   checks.
3. **Stream lifecycle.** The first value comes immediately; unsubscribe removes the listener.
   Pinned by the Task 2 web code review of the `StreamHandle` drop, and the Task 5 caption
   updates.
4. **Android Activity recreation** (a uiMode change recreates it). The core stream must keep
   emitting. Pinned by Task 5: toggle `cmd uimode` twice and check the caption follows both.
5. **Forced mode.** `Light`/`Dark` ignore the OS for rendering, but the caption (the OS value)
   still changes. Pinned by the Task 5 checks in forced Dark.

---

### Task 1: ABI + core API

**Files:**
- `mobiler-ui/src/lib.rs`: `Appearance` enum; `Scaffold.appearance` as the last field; the four
  test Scaffold literals (~935-997) get `appearance: None` (one test uses `Some(Appearance::System)`).
- `mobiler-core/src/lib.rs`:
  - re-export `Appearance`
  - the `scaffold`/`nav_scaffold` literals (~1167, 1176, 1196) get `appearance: None`
  - every `with_*` builder that rebuilds a Scaffold carries `appearance` through (the compiler
    lists them)
  - new `with_appearance`, `Cx::system_appearance`, `Cx::subscribe_appearance`, and tests
- `mobiler-web/src/lib.rs:~1980`: the Scaffold destructure binds `appearance` (used in Task 2).

**Interfaces (produces):**
- `mobiler_ui::Appearance { Light, Dark, System }` (`Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq`, `#[repr(C)]`)
- `Widget::Scaffold { …, labels, appearance: Option<Appearance> }`
- `mobiler_core::with_appearance(Widget, Appearance) -> Widget`
- `Cx::system_appearance(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static)` → `plugin("device", "appearance", "")`
- `Cx::subscribe_appearance(&mut self, key: impl Into<String>, on_event: impl Fn(PluginResponse) -> E + Send + 'static)` → `subscribe(key, "appearance", "changes", "", on_event)`
- Swift: `case .scaffold(title, body, tabs, back, darkMode, theme, fab, sheet, onRefresh, refreshing, route, depth, labels, appearance)`, which is 14 positions.

- [ ] **Step 1: Failing tests** (mobiler-core test module):

```rust
    #[test]
    fn with_appearance_sets_it_on_a_scaffold_and_ignores_other_roots() {
        let s = with_appearance(scaffold("T", false, vec![], text("x")), Appearance::System);
        assert!(matches!(s, Widget::Scaffold { appearance: Some(Appearance::System), .. }));
        assert!(matches!(scaffold("T", true, vec![], text("x")), Widget::Scaffold { appearance: None, .. }));
        assert!(matches!(with_appearance(text("x"), Appearance::Dark), Widget::Text { .. }));
    }

    #[test]
    fn with_theme_and_labels_keep_the_appearance() {
        let s = with_appearance(scaffold("T", false, vec![], text("x")), Appearance::Light);
        let s = with_labels(with_theme(s, Theme::default()), ShellLabels::default());
        assert!(matches!(s, Widget::Scaffold { appearance: Some(Appearance::Light), .. }));
    }

    #[test]
    fn appearance_query_and_stream_shapes() {
        let mut cx = Cx::<Ev>::default();
        cx.system_appearance(|_| Ev::Tap);
        let (call, _) = &cx.requests[0];
        assert_eq!((call.plugin.as_str(), call.op.as_str(), call.input.as_str()), ("device", "appearance", ""));
        cx.subscribe_appearance("appr", |_| Ev::Tap);
        let (s, _) = &cx.streams[0];
        assert_eq!((s.key.as_str(), s.plugin.as_str(), s.op.as_str(), s.input.as_str()), ("appr", "appearance", "changes", ""));
    }
```

  Also extend mobiler-ui's `widget_round_trips` Scaffold cases so one of them carries
  `appearance: Some(Appearance::System)`.

- [ ] **Step 2: RED.** Run `cargo test -p mobiler-core 2>&1 | grep -E '^error' | sort | uniq -c`.
  Expected: missing `Appearance`/`with_appearance`/methods, plus a missing `appearance` field.

- [ ] **Step 3: Implement.**

  mobiler-ui, after `ShellLabels`:

```rust
/// How the scaffold picks light or dark. `Light`/`Dark` force it; `System` follows the OS live (the
/// shell switches palette sets itself, no core round-trip). Set with `with_appearance`.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Appearance { Light, Dark, System }
```

  Add this as the last field of `Widget::Scaffold`:

```rust
        /// `Some` overrides `dark_mode` (see [`Appearance`]); `None` = `dark_mode` decides.
        appearance: Option<Appearance>,
```

  mobiler-core, next to `with_labels`:

```rust
/// Choose Light / Dark / System for the scaffold. `System` follows the OS live on every shell and
/// switches the theme palette's light/dark set by itself; `dark_mode` is then ignored.
#[must_use]
pub fn with_appearance(widget: Widget, appearance: Appearance) -> Widget {
    match widget {
        Widget::Scaffold { title, body, tabs, back, dark_mode, theme, fab, sheet, on_refresh, refreshing, route, depth, labels, .. } => {
            Widget::Scaffold { title, body, tabs, back, dark_mode, theme, fab, sheet, on_refresh, refreshing, route, depth, labels, appearance: Some(appearance) }
        }
        other => other,
    }
}
```

  In `impl<E> Cx<E>`, next to `device_locale`:

```rust
    /// The OS light/dark setting (`"light"` / `"dark"` in `response.output`) via the built-in
    /// `device` capability — the system's value even while the app forces one with `with_appearance`.
    pub fn system_appearance(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("device", "appearance", "", then);
    }

    /// Subscribe to the OS light/dark setting: `on_event` gets `"light"` / `"dark"` once right away,
    /// then on every change. Stop it with [`unsubscribe`](Self::unsubscribe)`(key)`.
    pub fn subscribe_appearance(&mut self, key: impl Into<String>, on_event: impl Fn(PluginResponse) -> E + Send + 'static) {
        self.subscribe(key, "appearance", "changes", "", on_event);
    }
```

  Fix every compile error by adding `appearance: None` to the base constructors and binding and
  carrying `appearance` in each rebuilding `with_*`. Add `Appearance` and `with_appearance` to the
  exports. In mobiler-web's Scaffold destructure, bind `appearance`. Task 2 uses it; until then,
  prefix it `_appearance` to stay warning-free.

- [ ] **Step 4: GREEN + wider compile.** Run:

```bash
cargo test -p mobiler-ui -p mobiler-core 2>&1 | grep -E 'test result|FAILED'
cargo clippy -p mobiler-core --all-targets -- -D warnings 2>&1 | tail -1
(cd mobiler-web && cargo check -q --target wasm32-unknown-unknown)
for d in demos/coffee demos/todo demos/saldo demos/barbershop demos/fullstack-todo demos/fullstack-sqlx; do (cd $d && CARGO_TARGET_DIR=$PWD/target cargo check -q --workspace 2>&1 | grep -E '^error' | head -3); done
```

  Expected: pass, and no `error`.

- [ ] **Step 5: Swift positional patterns in every demo.** Every `.scaffold(` pattern in
  `demos/*/iOS/Sources/{Core,Render}.swift` and `demos/fullstack-todo/mobile/iOS/Sources/…`
  (10 sites) gets one more trailing position:
  - Core.swift: `…, labels, _)` or `…, labels)` becomes `…, labels, _)`.
  - Render.swift: `…, let depth, _)` becomes `…, let depth, _, _)`.
  - Barbershop's Render.swift binds `let appearance` instead of the last `_`, for Task 4.

  Grep afterwards: each pattern now has 14 positions.

- [ ] **Step 6: Commit:** `feat(ui)!: Scaffold.appearance (Light/Dark/System) + cx.system_appearance / subscribe_appearance`

---

### Task 2: Web shell

**Files:** `mobiler-web/src/lib.rs` (the scaffold render, `perform` device branch, stream
dispatch, `StreamHandle`, app start).

**Interfaces:**
- Consumes: the Task 1 ABI.
- Produces:
  - `fn os_dark() -> bool` (`matchMedia('(prefers-color-scheme: dark)').matches`, false when
    unavailable)
  - `fn resolve_dark(appearance: Option<Appearance>, dark_mode: bool) -> bool`

- [ ] **Step 1: Failing test:**

```rust
    #[test]
    fn resolve_dark_follows_the_appearance() {
        assert!(!resolve_dark(None, false) && resolve_dark(None, true));
        assert!(!resolve_dark(Some(Appearance::Light), true));
        assert!(resolve_dark(Some(Appearance::Dark), false));
    }
```

  `System` calls `os_dark()`, which is `false` on the host test target; see the implementation.
  Run `cargo test`. RED: `resolve_dark` is missing.

- [ ] **Step 2: Implement.**
  - `os_dark()`:
    `web_sys::window().and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten()).is_some_and(|m| m.matches())`.
    The host test target has no window, so it returns false. Check the `MediaQueryList` web-sys
    feature in `mobiler-web/Cargo.toml` and add it if missing.
  - `resolve_dark(a, dm)`:
    `match a { Some(Appearance::Light) => false, Some(Appearance::Dark) => true, Some(Appearance::System) => os_dark(), None => dm }`.
  - Scaffold render: compute `let dark = resolve_dark(*appearance, *dark_mode);` and use `dark`
    everywhere `*dark_mode` was used: the `theme-dark` class, `theme_css(t, dark)` and
    `page_bg`. The confirm-portal copy already reads the class and style from the scaffold.
  - **Live re-render:** in `shell::<A>()`, after `send` is set up, install **one**
    `matchMedia(...)` `change` listener. It calls `set_view.set(core.view())`, which re-renders the
    current view so `os_dark()` is re-read. Keep the closure alive with `.forget()`; it is
    page-lifetime, like the shell itself.
  - **Device op:** in `perform`'s device branch, `call.op == "appearance"` answers
    `if os_dark() { "dark" } else { "light" }`.
  - **Stream:** in the stream dispatch, add `("appearance", "changes")`:
    - get the `MediaQueryList`
    - emit the current value
    - add a `change` listener closure that emits `"dark"`/`"light"` from `e.matches()` (a
      `MediaQueryListEvent`; add the web-sys feature)
    - return a new `StreamHandle::Appearance(AppearanceStream { mql, _onchange })`
  - **`AppearanceStream`** gets a `Drop` impl that calls `remove_event_listener_with_callback`,
    mirroring `SystemStream`'s drop.

- [ ] **Step 3: GREEN.** Run `cd mobiler-web && cargo test && cargo clippy --all-targets -- -D warnings && cargo check --target wasm32-unknown-unknown`.

- [ ] **Step 4: Commit:** `feat(web): Appearance — resolve dark from System/Light/Dark, live OS changes, appearance query + stream`

---

### Task 3: Android (barbershop)

**Files:** `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt`, `Core.kt`.

**Interfaces:** consumes the generated `Widget.Scaffold.appearance: Appearance?` (`Appearance.LIGHT/DARK/SYSTEM`).

- [ ] **Step 1: Resolve dark** (MainActivity ~294). Replace
  `val dark = (view as? Widget.Scaffold)?.darkMode ?: isSystemInDarkTheme()` with:

```kotlin
    val scaffold = view as? Widget.Scaffold
    val osDark = isSystemInDarkTheme()
    // Appearance: Light/Dark force the mode, System follows the OS (recomposes on its change);
    // none → dark_mode, as before. A non-Scaffold root follows the OS, as before.
    val dark = when (scaffold?.appearance) {
        Appearance.LIGHT -> false
        Appearance.DARK -> true
        Appearance.SYSTEM -> osDark
        null -> scaffold?.darkMode ?: osDark
    }
```

  Add the `Appearance` import from the shared types.

- [ ] **Step 2: Device op + stream** (Core.kt).
  - `DevicePlugin`: add
    `"appearance" -> PluginResponse(true, if ((android.content.res.Resources.getSystem().configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) == android.content.res.Configuration.UI_MODE_NIGHT_YES) "dark" else "light")`.
  - New class, registered as `"appearance" to AppearancePlugin(application)` in the plugin map:

```kotlin
/** Built-in `appearance` stream: the OS light/dark setting — the current value first, then each
 *  change. Application-level callbacks, so it survives Activity recreation on a uiMode change. */
class AppearancePlugin(private val app: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse =
        PluginResponse(false, "appearance is a streaming capability — use cx.subscribe_appearance")
    override fun subscribe(op: String, input: String): kotlinx.coroutines.flow.Flow<PluginResponse> =
        kotlinx.coroutines.flow.callbackFlow {
            fun current(c: android.content.res.Configuration) =
                if ((c.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) == android.content.res.Configuration.UI_MODE_NIGHT_YES) "dark" else "light"
            var last = current(android.content.res.Resources.getSystem().configuration)
            trySend(PluginResponse(true, last))
            val cb = object : android.content.ComponentCallbacks {
                override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
                    val now = current(newConfig)
                    if (now != last) { last = now; trySend(PluginResponse(true, now)) }
                }
                @Deprecated("") override fun onLowMemory() {}
            }
            app.registerComponentCallbacks(cb)
            awaitClose { app.unregisterComponentCallbacks(cb) }
        }
}
```

  Match the exact `MobilerPlugin` interface and `SystemPlugin` style in the file. If
  `onLowMemory`'s deprecation annotation doesn't compile, drop the annotation.

- [ ] **Step 3: Build.** Run `mobiler build android` in `demos/barbershop` (background).
  Expected: an APK, with no `e:` lines.

- [ ] **Step 4: Commit:** `feat(barbershop/android): appearance — resolve dark, device op, appearance stream`

---

### Task 4: iOS (barbershop) + iOS 17 in the demos

**Files:**
- `demos/barbershop/iOS/Sources/Render.swift`, `Core.swift`
- `demos/*/iOS/project.yml` and `demos/fullstack-todo/mobile/iOS/project.yml`: `iOS: "16.0"` → `"17.0"`

**Interfaces:** consumes the generated `Appearance` (`.light/.dark/.system`) and the 14-position
`.scaffold` pattern.

- [ ] **Step 1: Deployment target.** `sed -i 's/iOS: "16.0"/iOS: "17.0"/'` on the five demo
  `project.yml`. Grep afterwards to confirm.

- [ ] **Step 2: Resolve dark.**
  - In `ScaffoldView`, add `let appearance: Appearance?` (passed from the render arm) and
    `@Environment(\.colorScheme) private var systemScheme`.
  - Compute:

```swift
    // Light/Dark force the mode; System follows the OS (no preferredColorScheme → SwiftUI tracks it live).
    private var resolvedDark: Bool {
        switch appearance {
        case .some(.light): return false
        case .some(.dark): return true
        case .some(.system): return systemScheme == .dark
        case .none: return darkMode
        }
    }
```

  - Replace `.preferredColorScheme(darkMode ? .dark : .light)` with
    `.preferredColorScheme(appearance == .system ? nil : (resolvedDark ? .dark : .light))`.
  - At the top of `body`, add
    `let _ = { ActivePalette.current = ActiveTheme.current?.palette.map { resolvedDark ? $0.dark : $0.light } }()`.
    This sets the global before children render. In the `System` case, `systemScheme` is the live
    OS scheme, because the modifier doesn't force one.
  - Core.swift's resolution (per render) stays for the non-System cases. It must use the same
    rule, so make Core.swift compute
    `dark = appearance == .dark ? true : appearance == .light ? false : appearance == .system ? (UITraitCollection.current.userInterfaceStyle == .dark) : darkMode`.
  - `ScaffoldView`'s body pass then refines it with the environment value.

- [ ] **Step 3: Device op + stream** (Core.swift).
  - `DevicePlugin`: `case "appearance": return PluginResponse(ok: true, output: AppearanceBridge.osDark() ? "dark" : "light")`.
  - Add:

```swift
/// The OS light/dark setting: the active window scene's trait (system-level — a SwiftUI
/// preferredColorScheme override applies below the scene). iOS 17 trait-change registration.
@MainActor
final class AppearanceBridge {
    static let shared = AppearanceBridge()
    private var sink: (@Sendable (String) -> Void)?
    private var registration: (any UITraitChangeRegistration)?
    static func scene() -> UIWindowScene? {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first { $0.activationState == .foregroundActive }
            ?? UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
    }
    static func osDark() -> Bool { (scene()?.traitCollection.userInterfaceStyle ?? UITraitCollection.current.userInterfaceStyle) == .dark }
    func attach(_ s: @escaping @Sendable (String) -> Void) {
        sink = s
        s(Self.osDark() ? "dark" : "light")
        registration = Self.scene()?.registerForTraitChanges([UITraitUserInterfaceStyle.self]) { [weak self] (scene: UIWindowScene, _: UITraitCollection) in
            self?.sink?(scene.traitCollection.userInterfaceStyle == .dark ? "dark" : "light")
        }
    }
    func detach() {
        if let r = registration { Self.scene()?.unregisterForTraitChanges(r) }
        registration = nil; sink = nil
    }
}

enum AppearanceStream {
    static func run(emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await MainActor.run { AppearanceBridge.shared.attach(sink) }
        await withTaskCancellationHandler {
            while !Task.isCancelled { try? await Task.sleep(nanoseconds: 1_000_000_000) }
        } onCancel: {
            Task { @MainActor in AppearanceBridge.shared.detach() }
        }
    }
}
```

  - Register it in `Plugins.subscribe`: `case "appearance": await AppearanceStream.run(emit: emit)`.
  - Because the scene's trait fires for its own changes, emit only when the value differs from
    the last one. Keep a `last: String?` in the bridge, and apply the same guard in `attach`.

- [ ] **Step 4: Compile scrutiny** (no local Swift).
  - Check the `registerForTraitChanges` closure signature (iOS 17: `(Self, UITraitCollection) -> Void`).
  - Check that the `UITraitChangeRegistration` type name is right. If unsure, store it as
    `Any?`-free `(any UITraitChangeRegistration)?`, which is the iOS 17 protocol.
  - Check the `@MainActor` usage from `DevicePlugin` (already `@MainActor`).
  - CI is the gate.

- [ ] **Step 5: Commit:** `feat(barbershop/ios): appearance — resolve dark, device op, appearance stream; demos target iOS 17`

---

### Task 5: Barbershop demo + acceptance

**Files:** `demos/barbershop/app-core/src/lib.rs`.

- [ ] **Step 1: Model + UI.**
  - Replace `light_mode: bool` with `appearance: Appearance` (default `Appearance::Dark`) and
    `system_appearance: String`.
  - Home: replace the toggle with
    `segmented(vec![segment("Light", a == Light, Msg::SetAppearance(Light)), segment("Dark", …), segment("System", …)])`.
    Use the real `Segment` builder name from mobiler-core. Add
    `caption(format!("System: {}", model.system_appearance))`.
  - `view`: `with_appearance(scaffold(title_text, true, tabs, body), model.appearance)`.
    `dark_mode` is ignored once an appearance is set.
  - `init`: `cx.subscribe_appearance("appearance", |r| Msg::SystemAppearance(r.as_text().unwrap_or_default().to_string()));`
  - `update`: set the fields. `Msg` needs `SetAppearance(Appearance)`. `Appearance` implements
    Serialize/Deserialize, so it works inside the token.
  - Remove `light_mode` input handling. Update the barbershop test from the palette task:
    - default renders `appearance: Some(Dark)`
    - `SetAppearance(Light)` → `Some(Light)`
    - `init` subscribes to (`"appearance"`, `"changes"`)

- [ ] **Step 2: Tests + clippy:**
  `cd demos/barbershop && CARGO_TARGET_DIR=$PWD/target cargo test -q -p barbershop-core && cargo clippy -q --workspace --all-targets -- -D warnings`.

- [ ] **Step 3: Web acceptance** (CDP, per-run port, served `demos/barbershop/web/dist`):
  1. Emulate `prefers-color-scheme: dark` (`Emulation.setEmulatedMedia { features: [{name: 'prefers-color-scheme', value: 'dark'}] }`).
     Click **System**. Expect `.scaffold` background `#231f20` and the caption "System: dark".
  2. Emulate `light`. Expect `#faf7f0` without a click, the caption "System: light", and `<html>`
     `--bg` light.
  3. Click **Dark**, then emulate `light`. Expect the background to stay `#231f20` while the
     caption says "System: light".
  4. Expect no exceptions.
  5. Coffee/todo web pixel-identical to a `main`-worktree build (`getbbox() == None`).

- [ ] **Step 4: Android acceptance** (AVD, `mobiler dev`):
  1. Tap **System** (the segment bounds via uiautomator).
  2. `adb shell cmd uimode night yes`: expect the pixel at the background to be `#231f20`, light
     status-bar icons, and the caption "System: dark".
  3. `cmd uimode night no`: expect `#faf7f0` and "System: light". This also covers Activity
     recreation, because the uiMode change recreates the Activity.
  4. Tap **Dark**, then `night no`: the background stays `#231f20`, and the caption says
     "System: light".
  5. Coffee screenshot vs a baseline taken from a `main`-worktree build: expect only
     status-bar/clock differences. Freeze the clock with demo mode.
  6. Kill the emulator.

- [ ] **Step 5: Commit:** `feat(barbershop): Light · Dark · System segmented + live system-appearance caption`

---

### Task 6: Docs, review, PR

- [ ] NOTES.md section "System appearance (2026-09-28)" covering: the resolution rule, the stream
  semantics, the iOS 17 decision, the per-shell mechanisms, the forced-mode OS reporting, and the
  template port list (barbershop's shells + `project.yml`).
- [ ] Add `cx.system_appearance` / `cx.subscribe_appearance` to `capabilities.json`, then run
  `cargo run -q -p xtask -- gen-readme` and `--check`.
- [ ] Fresh whole-branch review (fix pass as before).
- [ ] ship-pr: all 25 checks, including every demo's iOS lane (the 14-position patterns and
  iOS 17). Squash-merge. No publish.
- [ ] Update memory `moj-termin-design-release` and `start.md`.
