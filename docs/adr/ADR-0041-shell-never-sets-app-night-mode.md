# ADR-0041: The shells never set an app-level night mode; the Android system splash follows the OS appearance, and an app that wants its own choice in the first frame keeps it in `cx.save` state

Status:        Accepted
Date decided:  2026-10-01
Deciding PRs:  #264
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt (OS appearance read from the configuration's uiMode), mobiler-core/src/lib.rs (`with_appearance` docs)
Conformance:   xtask/tests/adr_conformance.rs::adr_0041_no_shell_sets_an_app_level_night_mode

## 1. Context (The Problem)

ADR-0020 lets an app choose Light, Dark or System on the scaffold. Under System the shell follows the
OS, and the OS value is readable by `cx.system_appearance` and its stream. On Android the shell reads
that value from the configuration's `uiMode` night bits.

The appointments team (Moj Termin) reported (request of 2026-10-01, a temporary request document, so
quoted here): "The core reads the stored choice (`securestore`) during `init`. Until the reply arrives
the scaffold says System, so someone who chose Dark on a light phone sees a light frame first, then
dark." Their acceptance check: "An app set to Dark on a light phone opens dark from the first frame."

Two things are behind it:

- **The app's own read is late.** The shells send `AppInfo`, `Restore` and `Start` synchronously before
  their first frame (Android's Core constructor, iOS's `Core.init`, web's `shell`), so the first frame
  shows the model after `Start`. A `securestore` reply comes after that frame.
- **The launch window is the OS's.** Android draws its starting window and (12+) system splash from
  the app's theme before the app runs, in the OS night mode.

## 2. Hypothesis

If the shells never set an app-level night mode, and apps keep their appearance choice in `cx.save`
state, then:

- the first frame already shows the app's choice, on every shell;
- the configuration the Android shell reads the OS appearance from stays the OS's, so the System
  appearance query and stream (ADR-0020) report the OS, not the app's choice;
- the cost is stated, not hidden: on Android the system splash follows the OS.

### 2.1. Refutation Conditions

- **Condition 1 — no app-level night mode.** No Kotlin source in the template or a demo calls
  `UiModeManager.setApplicationNightMode` or `AppCompatDelegate.setDefaultNightMode`.
  - **Validation Metric:** `adr_0041_no_shell_sets_an_app_level_night_mode` in
    `xtask/tests/adr_conformance.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — set the app's choice as the application night mode** `[reconstructed]`
  `UiModeManager.setApplicationNightMode` (API 31+) makes the system splash follow the app. Rejected:
  it rewrites the configuration the shell reads the OS appearance from, so under a forced Dark the
  System query and stream would report the app's choice instead of the OS's, and every change recreates
  the activity. It also does nothing below API 31.
- **Option B — a shell-side "last appearance" hint** `[recorded: Moj Termin request, 2026-10-01, quoted here]`
  The team's preferred option: "A small, non-secret, synchronous app preference the shell reads before
  the first frame (e.g. `cx.initial_appearance()`, persisted by the shell when the scaffold's
  `appearance` changes)." Rejected: the first frame already reflects the model after `Start`, so the
  hint adds nothing there, and on Android the only surface before the first frame is the system launch
  window, which a running app cannot paint.
- **Option C — the app keeps its choice in `cx.save` state; the shells set no night mode** `[recorded: docs/superpowers/specs/2026-10-01-dark-theme-gaps-design.md, decision 2]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. The guidance is in the `with_appearance` doc comment and the `mobiler-core` README. The
launch window itself follows the OS night mode through `values-night` themes (same release).

Native dialogs the shell opens follow the model, not the OS alone: the Android date and time pickers
get a context whose night mode is the app's resolved appearance (`pickerContext` in `Core.kt`). That
override is local to the dialog, so the application's configuration stays the OS's.

**Mutation proof:**
- Adding `(getSystemService(UI_MODE_SERVICE) as android.app.UiModeManager).setApplicationNightMode(android.app.UiModeManager.MODE_NIGHT_YES)`
  to the template's `MainActivity.onCreate` failed the test: "ADR-0041: …/MainActivity.kt:267 calls
  setApplicationNightMode".
- The same line with a trailing `// comment` failed it the same way: only whole-line comments are
  skipped.
- `val u = "https://x"; androidx.appcompat.app.AppCompatDelegate.setDefaultNightMode(2)` failed it:
  "… calls setDefaultNightMode" (a `//` inside a string does not hide the call).
- Reverting restored green. The walk skips symlinks (`walk_skips_symlinks_and_terminates`, red with a
  link loop before the fix).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the System appearance query and stream keep reporting the OS on Android, and no shell
  state can disagree with the model.
- **Negative:** an app that forces Dark on a light Android phone still shows a light system splash and
  launch window; the first frame after it is dark. iOS's launch screen likewise follows the OS.
- **Positive:** the Android pickers match the app: an app forced to Light on a dark phone keeps a
  light picker (the dark `values-night` theme would otherwise reach it), and an app forced to Dark gets a
  dark one.
- **Negative:** apps that already read their choice from `securestore` or `kv` must move it to their
  `cx.save` state to get a correct first frame.
- **Negative:** the conformance test scans Kotlin sources only; a plugin or app code outside them could
  still call the API.
