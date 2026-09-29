# ADR-0020: The app chooses Light, Dark or System on the scaffold; under System the shell follows the OS live without a core update, and the OS value is readable by query and stream

Status:        Accepted
Date decided:  2026-09-28
Deciding PRs:  #223
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Appearance, Widget::Scaffold.appearance), mobiler-core/src/lib.rs (with_appearance, Cx::system_appearance, Cx::subscribe_appearance), mobiler-web/src/lib.rs (resolve_dark, os_dark), template shells (iOS Core.swift applyWindowStyle + AppearanceBridge; Android MainActivity.kt appearance resolution, Core.kt appearance stream)
Conformance:   mobiler-core/src/lib.rs::with_appearance_sets_it_on_a_scaffold_and_ignores_other_roots, mobiler-core/src/lib.rs::appearance_query_and_stream_shapes, mobiler-web/src/lib.rs::resolve_dark_follows_the_appearance

## 1. Context (The Problem)

The Moj Termin design offers Light, Dark and System. Before PR #223, `Scaffold.dark_mode: bool`
decided the mode, and the shells disagreed about the OS: iOS forced a colour scheme, Android
ignored the OS once a Scaffold was the root, and the web had no `prefers-color-scheme` handling. No
capability reported the OS setting, and no event said it changed. So an app could not offer
"follow the system" at all.

## 2. Hypothesis

If the app states its choice as `Scaffold.appearance: Option<Appearance>` (`Light`, `Dark`,
`System`), and each shell resolves one dark flag from it (`System` = the OS value, read live by the
shell; `None` = `dark_mode`, as before), then:

- an app offers all three modes with one field;
- under `System` an OS switch re-themes the app immediately, with no core update in between;
- apps that never set `appearance` behave exactly as before;
- the app can still learn the OS value (`cx.system_appearance`, `cx.subscribe_appearance`), and
  that value is the OS setting even while the app forces a mode.

### 2.1. Refutation Conditions

- **Condition 1 — the choice rides on the scaffold, and `None` is the default.**
  `with_appearance` sets it on a Scaffold root and is a no-op on any other widget; `scaffold(…)`
  leaves it `None`.
  - **Validation Metric:** `with_appearance_sets_it_on_a_scaffold_and_ignores_other_roots` in
    `mobiler-core/src/lib.rs`.
- **Condition 2 — the resolution rule.** `Light` → light, `Dark` → dark, `None` → `dark_mode`,
  whatever the other inputs are.
  - **Validation Metric:** `resolve_dark_follows_the_appearance` in `mobiler-web/src/lib.rs`. The
    `System` branch reads `prefers-color-scheme`, which only exists on wasm; it was checked in the
    headless-Chrome run recorded in PR #223. The Android and iOS resolution is checked by review.
- **Condition 3 — the query and the stream are plain capability calls.** The query is `device` /
  `appearance`; the stream is the keyed `appearance` / `changes` stream (ADR-0006).
  - **Validation Metric:** `appearance_query_and_stream_shapes` in `mobiler-core/src/lib.rs`.
- **Condition 4 — the query reports the OS, not the forced mode.** Checked on web and Android in
  PR #223 ("Forced Dark/Light stay put while the caption reports the OS"). No unit test.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep `dark_mode: bool`; apps subscribe to the OS value and re-render** `[reconstructed]`
  Needs only the stream, but every app would write the same plumbing, the first frame would not
  know the OS value, and each OS switch would wait for a core round-trip.
- **Option B — shells always follow the OS** `[reconstructed]`
  Simplest for the shells, but it removes the forced Light and Dark the design asks for
  (`[recorded: docs/superpowers/specs/2026-09-28-system-appearance-design.md]` "The design offers
  Light, Dark and System.").
- **Option C — an optional three-way `Appearance` on the scaffold, resolved by the shell** `[recorded: docs/superpowers/specs/2026-09-28-system-appearance-design.md, decisions 1–4 ("`System`: the shell follows the OS live, with no core round-trip."; the query answers "the **OS** value, whatever the app forces"); PR #223]`
  Chosen.
- **iOS mechanism — SwiftUI `preferredColorScheme`** `[recorded: PR #223 ("`preferredColorScheme(nil)` doesn't reliably release a forced scheme"); mobiler/templates/iOS/Sources/Core.swift, the `applyWindowStyle` doc comment]`
  The spec first planned to keep forcing the scheme this way. PR #223 set a window-level
  `overrideUserInterfaceStyle` instead. The template's comment gives both reasons: "SwiftUI's
  `preferredColorScheme`, which doesn't reliably let go when set back to nil", and the window
  override sits below the scene, "so the scene's trait always stays the OS value the appearance
  query reads".

## 4. Decision & Rationale for Corroboration

Option C. `Widget::Scaffold` gained `appearance: Option<Appearance>`, then its last field (PR #223;
the append is covered by ADR-0008). Each shell resolves one dark flag with the rule
`Light → false, Dark → true, System → the OS, None → dark_mode`, and everything that used
`dark_mode` uses that flag: the palette set (ADR-0019), the
web `theme-dark` class, the Android colour scheme and system bars, and the iOS window style.

- **Web:** `resolve_dark` reads `matchMedia('(prefers-color-scheme: dark)')` and re-renders on its
  `change` event.
- **Android:** `isSystemInDarkTheme()` under `System`. The stream uses application
  `registerComponentCallbacks`, so it survives Activity recreation.
- **iOS:** `applyWindowStyle` sets each window's `overrideUserInterfaceStyle` (`.unspecified` under
  `System`). `AppearanceBridge` reads the window scene's trait and registers for its changes, which
  needs iOS 17 (ADR-0021).

PR #223 records the web (CDP `setEmulatedMedia`) and Android (`cmd uimode night`) checks: `System`
follows the OS both ways without a tap, and forced modes stay put while the stream reports the OS.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** one field gives an app all three modes, and a `System` switch costs no core update.
- **Positive:** the app still sees the OS value when it needs it (for example to label a "System"
  option), through the ordinary query and stream.
- **Negative:** two fields now describe the mode, `dark_mode` and `appearance`, with a precedence
  rule every shell must implement the same way. Only the web rule has a unit test.
- **Negative:** under `System` the core does not know which mode is on screen. An app whose logic
  depends on it must subscribe to the stream itself.
- **Negative:** shell code that caches the active mode must re-read it on every switch. PR #243
  fixed iOS text that kept the dark palette's colour after a switch to Light.
- **Negative:** the iOS OS-value tracking uses an iOS 17 API, which raised the deployment target
  (ADR-0021).
