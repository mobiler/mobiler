# App info at startup: `cx.app_info()`

**Request:** `docs/app-version.md` (appointments admin app, 2026-09-27).
**Target:** mobiler-ui 0.28.0 / mobiler-core 0.39.0 / mobiler-web 0.39.0, CLI 0.57.0.

## Problem

The app sends its store build number (`X-Client-Build`) on every request so the server can say
"update recommended / required". The core cannot read the platform's own version or build number
today, so the app keeps a hand-maintained Rust constant plus a tie-test against `Info.plist` and
`build.gradle.kts`. A CI lane that sets the build number at build time would make the constant lie.

## Decision

The request offers an async `device.app_info` op or a synchronous value the shell hands over before
`init`, and prefers the synchronous value. We ship **only the synchronous value**. It covers every
use of the async op (the value is known before `init`), so a second path would be duplicate shell
code in three places.

## Contract

### ABI (mobiler-ui)

A new `Action` variant, **appended last** so existing variant indices stay put:

```rust
/// The app's own version, sent once by the shell at startup, before `Restore`/`Start`.
AppInfo { version: String, build: String, platform: String, bundle_id: String },
```

An older shell never sends it, and nothing else changes for it.

### Core (mobiler-core)

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppInfo {
    pub version: String,   // iOS CFBundleShortVersionString / Android versionName
    pub build: String,     // iOS CFBundleVersion / Android longVersionCode, as text
    pub platform: String,  // "ios" | "android" | "web"; "" if the shell never sent it
    pub bundle_id: String, // iOS bundle identifier / Android packageName
}

impl<E> Cx<E> {
    /// The app's version/build as reported by the shell at startup. Set before `restore`/`init`,
    /// so `init` can use it for the first request. All fields are empty when the shell predates it.
    pub fn app_info(&self) -> &AppInfo;
    /// A `Cx` carrying `info`, for app unit tests.
    pub fn with_app_info(info: AppInfo) -> Self;
}
```

- `build` is a `String` because iOS allows dotted builds (`1.2.3`). Apps that need a number parse it.
- Storage: `MobilerShell` is stateless (the app's `A::default()` is rebuilt per update and the
  Model belongs to the app), so the value lives in a process-wide `static APP_INFO: RwLock<AppInfo>`
  in mobiler-core. `Action::AppInfo` replaces it. The last one sent wins, which keeps hot reload
  harmless. There is one core per process on every shell.
- `MobilerShell::update` builds each `Cx` with a clone of the global. `Cx::default()` stays empty
  and never reads the global, so unit tests are isolated from each other.
- `Action::AppInfo` does not call the app and produces no effects except the usual `Render`.
- `view` has no `Cx`. An app that shows the version on a screen copies it into its Model in `init`.

### Shells

Each shell sends `AppInfo` once, before the existing `Restore`/`Start`:

| Shell | version | build | platform | bundle_id |
|---|---|---|---|---|
| iOS (`Core.swift` init) | `CFBundleShortVersionString` | `CFBundleVersion` | `"ios"` | `Bundle.main.bundleIdentifier` |
| Android (`Core.kt` init) | `PackageInfo.versionName` | `PackageInfo.longVersionCode.toString()` | `"android"` | `packageName` |
| web (`mobiler-web` `run`) | `""` | `""` | `"web"` | `""` |

- A missing Info.plist key or a null `versionName` maps to `""`, never a crash.
- Android: minSdk is 34, so `longVersionCode` needs no compatibility shim.
- Web: there is no build manifest today, so the fields stay empty. The acceptance criterion only
  requires that web answers and never errors.

### Existing `device` ops

`model` and `locale` are unchanged.

## Rollout

- **mobiler-ui:** the variant, plus the Action round-trip test extended with it.
- **mobiler-core:** `AppInfo`, the global, `Cx::app_info` / `with_app_info`, handling in
  `MobilerShell::update`, a doc section, and tests:
  - `init` sees the info sent before `Start`.
  - The fields are empty when nothing was sent.
  - `with_app_info` works.
- **mobiler-web:** sends the web `AppInfo` before `Restore`.
- **Templates** (`mobiler/templates/iOS/Sources/Core.swift`,
  `mobiler/templates/Android/.../Core.kt`): send `AppInfo`. The template `mobiler-core` version
  string is bumped to 0.39.0.
- **Demos:** the same startup line in every demo shell (barbershop, coffee, saldo, todo,
  fullstack-todo). Consumers' version requirements are bumped.
- **Docs:** the capability/README entry for `cx.app_info()`; NOTES.md.

## Acceptance (from the request)

1. Android with `versionCode = 7` / `versionName = "1.0"` gives `build "7"`, `version "1.0"`,
   `platform "android"` and the package name. Verified on the AVD: a demo shows `cx.app_info()`
   from `init`, and the values match `build.gradle.kts`.
2. iOS gives `CFBundleVersion`, `"ios"` and the Info.plist bundle id. Only the macOS CI build checks
   this. A runtime check waits for the next simulator session.
3. Web answers `platform "web"` and never errors. Verified with the headless-chrome recipe.
4. `device` `model` / `locale` are unchanged.

## Out of scope

- An async `device.app_info` op.
- A web version manifest.
- System appearance, which is a separate request: `docs/system-appearance.md`.

## Amendment (2026-09-30)

The Android minimum is now API 26 (ADR-0039), so the note above that `longVersionCode` "needs no
compatibility shim" no longer holds: Core.kt reads `longVersionCode` on API 28+ and `versionCode`
below, and uses `PackageInfoFlags` only on API 33+.

