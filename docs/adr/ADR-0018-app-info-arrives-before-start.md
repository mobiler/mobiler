# ADR-0018: The app's version, build, platform and bundle id reach the core as an `Action::AppInfo` that every shell sends before `Restore` / `Start`; the core keeps it process-wide so `cx.app_info()` is already set in `init`

Status:        Accepted
Date decided:  2026-09-27
Deciding PRs:  #218 (ABI, core, web, demos), #219 (template shells)
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Action::AppInfo), mobiler-core/src/app_info.rs (APP_INFO), mobiler-core/src/lib.rs (AppInfo, Cx::app_info, Cx::with_app_info, MobilerShell::update), mobiler-web/src/lib.rs (fn shell: sends AppInfo first), template iOS Core.swift and Android Core.kt (startup sends AppInfo, then Restore, then Start)
Conformance:   mobiler-core/src/lib.rs::init_sees_app_info_sent_before_start, mobiler-core/src/lib.rs::app_info_action_does_not_touch_the_model, mobiler-core/src/lib.rs::later_app_info_replaces_earlier

## 1. Context (The Problem)

The appointments app sends its store build number on every request, so its server can say "update
recommended" or "update required". The core could not read the platform's version or build number.
The app kept a hand-maintained Rust constant and a test tying it to `Info.plist` and
`build.gradle.kts`. The spec: "A CI lane that sets the build number at build time would make the
constant lie." (`docs/superpowers/specs/2026-09-27-app-info-design.md`).

The value is needed in `init`, for the first request. It never changes while the process runs.

## 2. Hypothesis

If each shell sends `Action::AppInfo { version, build, platform, bundle_id }` once, before `Restore`
and `Start`, and the core stores it process-wide and puts a copy into every `Cx`, then:

- `init` and every later `update` can read `cx.app_info()` synchronously, with no request and no
  extra app state;
- a shell that predates it sends nothing, and the app sees empty strings, not an error;
- the action never reaches the app and changes no model.

### 2.1. Refutation Conditions

- **Condition 1 — `init` sees the value.** After `AppInfo` then `Start`, `init` reads the build.
  - **Validation Metric:** `init_sees_app_info_sent_before_start` in `mobiler-core/src/lib.rs`.
- **Condition 2 — the action is invisible to the app.** It doesn't call the app or touch the model.
  - **Validation Metric:** `app_info_action_does_not_touch_the_model`.
- **Condition 3 — the last value wins** (a hot reload may send it again).
  - **Validation Metric:** `later_app_info_replaces_earlier`.
- **Condition 4 — every shell sends it first.** Not tested: the order lives in each shell's
  startup code. Review, plus the Android emulator check recorded in PR #218 ("fresh scaffold with
  `versionCode = 7` shows `android 1.0 (7) rs.probe.appinfo` from `init`").

## 3. Considered Options & Rationale for Refutation

- **Option A — a hand-maintained constant in the app** `[recorded: docs/superpowers/specs/2026-09-27-app-info-design.md, Problem]`
  What the app had. It drifts from the real build number, and a CI lane that stamps the build
  would make it wrong.
- **Option B — an async `device.app_info` plugin op** `[recorded: same spec, Decision]`
  The request offered it alongside the synchronous value. The spec: "We ship **only the synchronous
  value**. It covers every use of the async op (the value is known before `init`), so a second path
  would be duplicate shell code in three places." `[reconstructed]` A request would also make
  `init` wait a round trip before its first request can carry the build.
- **Option C — keep it on `MobilerShell` or in the app's model** `[recorded: same spec, Core: "`MobilerShell` is stateless (the app's `A::default()` is rebuilt per update and the Model belongs to the app)"]`
  There is no framework-owned per-instance state to hold it, and the model belongs to the app.
- **Option D — a new `Action` variant sent first, stored in a process-wide static** `[recorded: same spec, Contract; PR #218 ("Stored process-wide (`MobilerShell` is stateless); last value wins; `Cx::default()` stays empty.")]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D. `Action::AppInfo` is appended as the last variant (ADR-0008), so existing indices stay put.
`MobilerShell::update` builds each `Cx` with `Cx::with_app_info(app_info::get())`. The
`AppInfo` arm only calls `app_info::set` and then emits the usual `Render`. `Cx::default()` stays
empty and never reads the global, so app unit tests are isolated, and `Cx::with_app_info` lets them
set a value.

The shells send it before `Restore` / `Start`: iOS from `Bundle.main` (`CFBundleShortVersionString`,
`CFBundleVersion`, bundle id), Android from `PackageInfo` (`versionName`, `longVersionCode`, package
name), and web with `platform: "web"` and the other fields empty. Missing values map to `""`. The
template half landed in #219, after the libraries were published (ADR-0009).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the build number is right by construction, and `init` has it for the first request.
- **Positive:** a shell that predates it keeps working; the app just sees empty fields.
- **Negative:** a process-wide static assumes one core per process. The spec: "There is one core
  per process on every shell." Two cores in one process would share, and overwrite, the value.
  The core's own tests that send the action must hold `APP_INFO_TEST_LOCK` to run in parallel.
- **Negative:** `view` has no `Cx`. An app that shows the version on screen must copy it into its
  model in `init`.
- **Negative:** web has no build manifest, so on web only `platform` is set.
- **Negative:** the pattern is a new `Action` variant, so it is an ABI change for exhaustive matches
  (ADR-0008) and needs the two-PR release (ADR-0009). It suits values known before `init` that never
  change; anything that changes at runtime belongs in a request or a stream (ADR-0002, ADR-0006).
