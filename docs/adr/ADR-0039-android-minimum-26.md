# ADR-0039: The Android minimum is API 26 (Android 8.0) in the template and every demo; shell and plugin code that needs a newer API is guarded by a version check

Status:        Accepted
Date decided:  2026-09-30
Deciding PRs:  #258
Supersedes:    none
Code anchor:   mobiler/templates/Android/{app,shared}/build.gradle.kts (`minSdk`), demos/*/Android/{app,shared}/build.gradle.kts, the template and demo Core.kt (app-info `packageInfo`/`appBuild`), the plugins' `Build.VERSION.SDK_INT` guards
Conformance:   xtask/tests/adr_conformance.rs::adr_0039_android_minimum_is_api_26

## 1. Context (The Problem)

Every generated Android project had `minSdk = 34`, so an app installed only on Android 14 and newer.
The value dates from the initial commit (729eb31, 2026-05-25) and was never recorded as a decision.
The Moj Termin team found it when a staging build failed to install on the owner's Android 13
Samsung, with Samsung's "There was a problem parsing the package". Their written request is not in
the repo, so it is quoted here `[recorded: Moj Termin team request, 2026-09-30, quoted here]`: "Moj Termin's users are salon staff on their own phones. Many of them are mid-range
Samsungs that have stopped getting OS updates, and Android 13 and older are still a large share of
phones in use." They asked for 26 (Android 8.0), or 28.

ADR-0021 records the iOS minimum (17.0). This is its Android counterpart.

## 2. Hypothesis

If the minimum is API 26 and every call to a newer API is behind a `Build.VERSION.SDK_INT` check,
then:

- apps install on Android 8.0 and newer, which covers practically every phone still in use;
- nothing the shells or bundled plugins do crashes on an older OS; a feature missing there
  degrades (for example no runtime notification prompt below Android 13, where none is needed),
  and a plugin that throws anyway answers `ok: false` (ADR-0013);
- Android 14+ behaviour is unchanged.

### 2.1. Refutation Conditions

- **Condition 1 — the minimum is 26.** Both gradle modules set `minSdk = 26`, in the template and
  in every demo.
  - **Validation Metric:** `adr_0039_android_minimum_is_api_26` in `xtask/tests/adr_conformance.rs`.
- **Condition 2 — no unguarded newer API.** Android Lint's `NewApi` check reports nothing at
  `minSdk = 26`. Checked on 2026-09-30 over barbershop (most plugins) and a scaffold with the other
  ten plugins installed. No CI job runs lint, so this is checked by running lint when a shell or
  plugin changes.
- **Condition 3 — it runs.** Barbershop installs and runs on an API 26 and an API 33 emulator
  (see §4).

## 3. Considered Options & Rationale for Refutation

- **Option A — keep 34** `[recorded: 729eb31 (the value, with no stated reason)]`
  Excludes Android 13 and older phones, which the Moj Termin team's users carry.
- **Option B — 28 (Android 9)** `[recorded: Moj Termin team request, 2026-09-30, quoted here: "**28 (Android 9)** would also do."]`
  Would avoid the `longVersionCode` guard, but reaches fewer phones for one small guard.
- **Option C — below 26** `[reconstructed]`
  The template's launcher icons are adaptive (`mipmap-anydpi`, API 26) and would need PNG-only
  fallbacks, the plugins' pre-channel notification paths (below 26) have never run, and
  `securestore` needs 23. All for phones almost nobody still uses.
- **Option D — 26** `[recorded: Moj Termin team request, 2026-09-30, quoted here: "Our suggestion is **26 (Android 8.0)**, which reaches practically every phone still in use."]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D. Lint at `minSdk = 26` found three calls, all in app-info's startup code:
`PackageManager.PackageInfoFlags` and its `getPackageInfo` overload (API 33) and
`PackageInfo.longVersionCode` (API 28). Core.kt now uses them behind `SDK_INT` checks, with the
deprecated `getPackageInfo(name, 0)` and `versionCode` below, in the template and every demo.
Everything else lint can see was already guarded: the Android 13 notification permission in push,
notifications and geofence, among others. No bundled library needs a higher minimum (the manifest
merge passes at 26).

Lint only sees API calls, not how permissions and platform services differ by version. A review of
all 31 plugins found three such gaps, fixed in the same PR:

- `bluetooth` declared only Android 12's `BLUETOOTH_SCAN` / `BLUETOOTH_CONNECT`. On Android 8–11 it
  needs the legacy `BLUETOOTH` / `BLUETOOTH_ADMIN`, now declared with `maxSdkVersion="30"` (plugin
  manifests accept `name;maxSdkVersion=N`). Without them it threw a `SecurityException`.
- `biometric` asked for `BIOMETRIC_STRONG | DEVICE_CREDENTIAL`, which Android 9–10 reject; below
  Android 11 it now asks for `BIOMETRIC_WEAK | DEVICE_CREDENTIAL`.
- `sqlite` uses the phone's own SQLite (3.18 on Android 8), so SQL features follow the OS. Saldo's
  settings UPSERT (needs 3.24) became `INSERT OR REPLACE`.

The shells' plugin dispatch now catches what a plugin throws, on both paths: a request answers
`ok: false`, and a stream ends with one `ok: false` event. That includes `LinkageError`
(`NoSuchMethodError`, `NoClassDefFoundError`), what calling a newer API on an older Android throws.
So a gap nobody has found yet degrades instead of crashing the app, as long as it throws inside the
plugin's call or flow; an exception in a platform callback the plugin registers is not covered.
`mobiler upgrade`'s drift report also names a plugin whose declared permissions are missing from the
app's manifest, so an existing app with `bluetooth` is told to re-add it. The CLI README's "Android versions"
table lists what older phones get. `mobiler upgrade` carries the new value into
existing apps through the merge of `build.gradle.kts` (ADR-0012).

**Mutation proof:**
- Setting the template's `app/build.gradle.kts` back to `minSdk = 34` failed the test: "ADR-0039:
  mobiler/templates/Android/app/build.gradle.kts must set minSdk = 26".
- Setting `demos/todo/Android/shared/build.gradle.kts` back to 34 failed it: "ADR-0039:
  demos/todo/Android/shared/build.gradle.kts must set minSdk = 26".
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** apps install on Android 8.0 and newer.
- **Negative:** every new shell or plugin API above 26 needs a version check, and nothing in CI
  enforces it. Lint does, when someone runs it; a CI lint job would close that.
- **Negative:** behaviour on old Android versions is tested by hand on emulators, not in CI.
- **Negative:** features that only exist on newer Android are absent or reduced on older phones:
  the system photo picker (the file picker instead), a fresh `geolocation` fix (last known location
  below 11), video picture-in-picture (12+), strong-only biometrics (11+). The CLI README's "Android
  versions" table is the list.
- **Negative:** apps using `sqlite` must write SQL the oldest supported SQLite (3.18) understands,
  or bundle their own SQLite.
- **Negative:** an existing app that edited its `minSdk` line gets a merge conflict on upgrade.
