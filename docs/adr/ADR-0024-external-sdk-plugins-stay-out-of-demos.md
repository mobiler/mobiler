# ADR-0024: A plugin that needs a per-app vendor config file (Firebase's `google-services.json` / `GoogleService-Info.plist`) is never installed in a committed demo; the demo carries only its Rust card, which reports "unavailable", and the plugin's native code is compile-checked outside CI

Status:        Accepted
Date decided:  2026-06-06
Deciding PRs:  #118 (first instance: `push`); #121 (2026-06-07) `push-firebase-only`; #141 (2026-06-09) `analytics`
Supersedes:    none
Code anchor:   mobiler/plugins/push/**, mobiler/plugins/push-firebase-only/**, mobiler/plugins/analytics/**, demos/barbershop/app-core/src/lib.rs (the Push and Analytics cards), the demos' Android `build.gradle.kts` and iOS `project.yml` (no Firebase entries)
Conformance:   none — a scoping rule for what the committed demos install; nothing tests that a demo lacks a plugin, so review enforces it

## 1. Context (The Problem)

The committed demos are CI lanes: each one builds for Android, iOS and web on every PR, so a
plugin installed in a demo gets its native code compiled for free. `push` (#118) was the first
plugin that could not simply be installed. Android FCM applies the `google-services` Gradle
plugin, and the build fails without a `google-services.json` in `Android/app/`. That file is
per-app and carries the app's Firebase keys, as the plugin itself says:
`[recorded: mobiler/plugins/push/android/PushPlugin.kt]` "carries your Firebase keys; can't be
bundled". `push-firebase-only` and `analytics` added the same need on iOS
(`GoogleService-Info.plist`).

## 2. Hypothesis

If vendor-config plugins stay out of every committed demo, and the demo keeps only the Rust card
that calls the plugin and shows its `ok: false` answer, then:

- no Firebase keys, real or dummy, live in the repo;
- the demos' CI lanes stay green without a Firebase project;
- the card still shows the app-side API and the graceful failure path (ADR-0013).

### 2.1. Refutation Conditions

- **Condition 1 — no committed demo registers a vendor-config plugin.** No demo's `Core.kt` or
  `Core.swift` registers `push`, `push-firebase-only` or `analytics`, and no
  `google-services.json` or `GoogleService-Info.plist` is tracked under `demos/`.
  - **Validation Metric:** review. There is no test; `git ls-files demos` and a grep of the
    registries show it today.
- **Condition 2 — the card degrades instead of breaking.** Without the plugin, barbershop's Push
  and Analytics cards show `unavailable: … (run `mobiler plugin add …`)`.
  - **Validation Metric:** review (and the web demo build, which compiles the cards but can't
    assert their text).

## 3. Considered Options & Rationale for Refutation

- **Option A — install the plugin in a demo with a real Firebase config** `[reconstructed]`
  Rejected: it commits one project's keys to a public repo and ties CI to that project.
- **Option B — install it with a dummy config file** `[reconstructed]`
  Not chosen. A dummy `google-services.json` was used for local scaffold builds (#118, #141), but
  never committed. Nobody wrote down why; the likely reasons are that a fake config in a demo looks
  like a working setup, and a dummy `GoogleService-Info.plist` would make the analytics
  config-absence guard pass and try to talk to Firebase.
- **Option C — Rust card only; the native plugin stays out** `[recorded: PR #118 body, "CI can't build push-installed Android (no `google-services.json` in CI), so committed demos carry the **Rust card only** — push is not installed in any committed shell."]`
  Chosen, and repeated in #141: "(the native plugin isn't installed in the demo, since the Android
  google-services Gradle plugin needs a committed `google-services.json`)" `[recorded: PR #141 body]`.

## 4. Decision & Rationale for Corroboration

Option C. As of this record, barbershop registers `geofence` and `oauth` (no vendor config), among others, but
not `push`, `push-firebase-only` or `analytics`, and no demo has a Firebase Gradle or SwiftPM
entry. Its Push and Analytics cards call the plugins and print the `ok: false` reason.

Because no CI lane compiles these plugins, each PR verified them by hand: a throwaway scaffold
with a dummy `google-services.json` built an Android APK (#118, #141), and #121 compiled
`FirebasePushPlugin.swift` on a Mac. #141 names the gap: "The iOS Firebase Swift isn't in a
committed demo, so (like `push-firebase-only`) it's **not CI-built**" `[recorded: PR #141 body]`.

`iap` is also not installed in any demo, though it needs no vendor config. Its PR (#120) does not
say why, so this record does not cover it.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** no secrets in the repo, and the demo lanes need no external account.
- **Positive:** the demos double as a check that a missing plugin fails soft (ADR-0013).
- **Negative:** the native code of `push`, `push-firebase-only` and `analytics` is never compiled
  in CI. A change that breaks it ships unless someone builds a scaffold by hand. This is a real
  gap: these plugins are also marked experimental and not device-tested end to end.
- **Negative:** barbershop is the reference shell (ADR-0015), but these plugins never run in it. A
  shell change that breaks them (for example to the `PushBridge` in `App.swift`) is caught only
  where the bridge itself compiles, not where the plugin uses it.
- **Negative:** each new vendor-config plugin repeats the manual verification. The rule doesn't
  say what that verification must include; each PR decides.
