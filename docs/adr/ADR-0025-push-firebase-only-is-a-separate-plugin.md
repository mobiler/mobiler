# ADR-0025: A second native implementation of a capability ships as a separate bundled plugin that registers under the same cx name, mutually exclusive with the default, never as an install option of one plugin

Status:        Accepted
Date decided:  2026-06-07
Deciding PRs:  #121 (first instance: `push-firebase-only` beside `push`); #147 (2026-06-09) repeats it for `geolocation-fused` beside `geolocation`; #203 (2026-09-16) makes the drift report tell the two apart
Supersedes:    none
Code anchor:   mobiler/plugins/push-firebase-only/**, mobiler/plugins/geolocation-fused/**, mobiler/src/plugin.rs (PluginCmd::Add, drifted, registered)
Conformance:   mobiler/src/plugin.rs::add_bundled_push_firebase_only_injects_spm_package_and_registers_under_push, mobiler/src/plugin.rs::add_bundled_geolocation_fused_registers_under_geolocation_with_play_services, mobiler/src/plugin.rs::drifted_does_not_report_an_alternative_plugin_sharing_file_names

## 1. Context (The Problem)

Some capabilities have two reasonable native implementations with different costs. `push` uses
native APNs on iOS and FCM on Android, so a backend speaks two APIs. Some apps want FCM on both
platforms and accept the Firebase iOS SDK to get it. `geolocation` uses Android's framework
`LocationManager` with no extra dependency; FusedLocationProvider is more accurate but needs Play
Services.

The app's Rust code should not care which one is installed. And `mobiler plugin add` takes only a
source (`PluginCmd::Add { source }`): a plugin is one static `mobiler-plugin.toml` with no install
options (ADR-0011).

## 2. Hypothesis

If each variant is its own bundled plugin that registers the **same cx name** (`"push"`,
`"geolocation"`) and is documented as mutually exclusive with the default, then:

- app code is identical whichever variant is installed; switching is a `plugin add`, not a code
  change;
- the default stays free of the heavy dependency, and the dependency is opt-in (the same spirit as
  ADR-0010);
- the plugin manifest and `plugin add` need no variant or option mechanism.

### 2.1. Refutation Conditions

- **Condition 1 — the variant answers to the default's cx name.** Installing
  `push-firebase-only` registers `case "push"` (handle and stream) on iOS and `"push" to` on
  Android; installing `geolocation-fused` registers `"geolocation"`.
  - **Validation Metric:** `add_bundled_push_firebase_only_injects_spm_package_and_registers_under_push`
    and `add_bundled_geolocation_fused_registers_under_geolocation_with_play_services` in
    `mobiler/src/plugin.rs`.
- **Condition 2 — the variants stay distinguishable to tooling.** They share cx names and some
  file names (`PushPlugin.kt`, `GeolocationPlugin.kt`), so "installed" can't mean "its files are
  present". A stale `push` must not also be reported as a stale `push-firebase-only`. This holds
  only for a pair whose registration lines differ: `push` and `push-firebase-only` do (iOS
  `PushPlugin` vs `FirebasePushPlugin`); `geolocation` and `geolocation-fused` do not (see §5).
  - **Validation Metric:** `drifted_does_not_report_an_alternative_plugin_sharing_file_names` in
    `mobiler/src/plugin.rs`.
- **Condition 3 — mutual exclusion.** Nothing mechanical enforces it. `plugin add` does not refuse
  the second variant; the manifests' `notes` and READMEs tell the user. Review. The notes say a
  second install fails at build; that is not verified. On Android the second install can change
  nothing at all, because an identical register line is skipped and the shared files are identical.

## 3. Considered Options & Rationale for Refutation

- **Option A — make Firebase-on-iOS the only `push`** `[recorded: mobiler/plugins/push-firebase-only/README.md (added in 7b8e595), "**Pick `push`** if you want iOS to stay free of the Firebase SDK (native APNs)."]`
  Rejected: "The Firebase iOS SDK is a **large SPM dependency** and lengthens iOS builds" (same
  README). Apps that don't want it would pay for it.
- **Option B — one plugin with an install-time option (e.g. a flag on `plugin add`)** `[reconstructed]`
  Not chosen. The manifest is static and `plugin add` has no options; a variant switch would need a
  new CLI mechanism, conditional sources and conditional injections. Nobody wrote this rejection
  down.
- **Option C — a separate plugin under a different cx name (e.g. `"push-fcm"`)** `[reconstructed]`
  Not chosen: app code would have to change to switch variants.
- **Option D — a separate plugin under the same cx name** `[recorded: PR #121 body, "Registers under the same cx name `"push"` — **app code is identical**"]`
  Chosen. #147 followed it for location "(the push/push-firebase-only precedent)", and the
  `geolocation-fused` manifest says why the default stays: "use the default `geolocation` for a
  dependency-free build." `[recorded: mobiler/plugins/geolocation-fused/mobiler-plugin.toml]`

## 4. Decision & Rationale for Corroboration

Option D. `push-firebase-only` (#121) swaps only the iOS side (`FirebasePushPlugin.swift` plus the
Firebase SwiftPM package); its Android side is a copy of `push`'s. `geolocation-fused` (#147)
swaps only the Android side; its iOS side is a copy of `geolocation`'s. Both register the default's
cx name, and both carry a "MUTUALLY EXCLUSIVE" note that `plugin add` prints.

Sharing a cx name and file names had one cost already. Before #203, `mobiler upgrade` counted a
plugin as installed when its files were present, so "apps with `push` installed were told to also
`plugin add push-firebase-only`, which collides at build." `[recorded: PR #203 body]` #203 made
"installed" mean registered on every platform the plugin declares.

The cited tests are pre-existing behavioural tests and are not re-proven by mutation (README,
adaptations).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app switches implementation without touching Rust; the default plugins stay
  dependency-light.
- **Positive:** no new CLI machinery: each variant is an ordinary manifest.
- **Negative:** `geolocation` and `geolocation-fused` have identical registration lines and share
  `GeolocationPlugin.kt`, so the drift report cannot tell them apart. An app with either installed
  can be told to `plugin add` the other, the #203 bug again. Found while writing this record; not
  yet fixed.
- **Negative:** mutual exclusion is a convention. `plugin add` installs the second variant without
  complaint, and there is no `plugin remove`; switching means removing the first variant's files
  and registration lines by hand.
- **Negative:** the shared half is a copy (Android `PushPlugin.kt`, iOS `GeolocationPlugin.swift`).
  A fix to one must be made to the other, and nothing checks they stay equal.
- **Negative:** any tool that asks "which plugin is installed" must look at registrations, not file
  names (#203). A new tool that forgets repeats the #203 bug.
