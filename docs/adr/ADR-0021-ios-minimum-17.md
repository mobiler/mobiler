# ADR-0021: The iOS deployment target is 17.0 in the template and every demo with an iOS project, so the iOS shell uses iOS 17 APIs without availability checks

Status:        Accepted
Date decided:  2026-09-28
Deciding PRs:  #223 (the decision, and the five demo `project.yml` files 16.0 → 17.0); #242 (2026-09-29, the template `project.yml`, CLI 0.58.0)
Supersedes:    none
Code anchor:   mobiler/templates/iOS/project.yml (`deploymentTarget`), demos/*/iOS/project.yml, mobiler/templates/iOS/Sources/Core.swift (AppearanceBridge, SnackbarHost)
Conformance:   .github/workflows/ci.yml job "iOS build (${{ matrix.app }})" (its demos/barbershop leg compiles a Core.swift that uses `registerForTraitChanges` and `@Observable` without `#available` — below iOS 17 that fails to compile)

## 1. Context (The Problem)

Until September 2026 the template and the demos targeted iOS 16.0. The Moj Termin design set
needed a System appearance mode that reports the OS setting live, even while the app forces Light
or Dark (docs/superpowers/specs/2026-09-28-system-appearance-design.md). The spec reads
the OS value from the key window scene's `traitCollection.userInterfaceStyle`, because "the SwiftUI
override applies below the scene", and watches it with `UIWindowScene.registerForTraitChanges`,
which is iOS 17 API. Every iOS 17 API used on a 16.0 target
needs an `#available` branch and a fallback.

## 2. Hypothesis

If the minimum is 17.0 everywhere, then:

- the shells can call iOS 17 APIs directly (`registerForTraitChanges` for the appearance stream,
  `@Observable` for the snackbar host), with no `#available` branches and no iOS 16 fallback to
  write or test;
- devices that can't run iOS 17 (iPhone 8, 8 Plus and X) can no longer install a mobiler app.

### 2.1. Refutation Conditions

- **Condition 1 — the target and the code agree.** The shell compiles with iOS 17 APIs ungated.
  Lowering barbershop's target, or using an API newer than 17 without a check, fails the iOS lane.
  - **Validation Metric:** `.github/workflows/ci.yml` job `iOS build (${{ matrix.app }})`, the
    `demos/barbershop` leg. The template's `project.yml` is not compiled for iOS by CI; its value is
    checked by review (see ADR-0015).

## 3. Considered Options & Rationale for Refutation

- **Option A — stay on 16.0 and gate iOS 17 APIs with `#available`** `[reconstructed]`
  Rejected in effect: every new iOS 17 use needs a second code path that CI's simulator (a current
  iOS) never runs.
- **Option B — raise the minimum to 17.0** `[recorded: docs/superpowers/specs/2026-09-28-system-appearance-design.md, decision 5, "**iOS minimum deployment target → 17.0** (user decision 2026-09-28: drop iPhone 8/8 Plus/X)." and "iOS 17 allows `UIWindowScene.registerForTraitChanges` for a live OS value"]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option B. PR #223 moved the five demo `project.yml` files ("`iOS/project.yml` moves to **iOS
17.0** (drops iPhone 8/8 Plus/X)") and added `AppearanceBridge` on `registerForTraitChanges`. The
spec kept the template on 16.0 until the release CLI PR, because the template ships only with the
published libraries (ADR-0009). PR #242 moved it ("palette, and Light/Dark/System appearance
(**iOS deployment target 17.0**)"). Existing apps get the new value through `mobiler upgrade`,
since `iOS/project.yml` is a merged file (ADR-0012).

Later work in the same release relied on it: the snackbar's `SnackbarHost` is `@Observable`, which
is iOS 17 Observation.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** shell code can use iOS 17 SwiftUI and UIKit APIs directly.
- **Negative:** apps built with CLI 0.58.0 or later, or upgraded to it, can't be installed on
  iPhone 8, 8 Plus or X. This is a compatibility break for any app with users on those devices.
- **Negative:** going back below 17.0 now means adding `#available` fallbacks for the appearance
  stream and the snackbar host at least. It is no longer a one-line change.
- **Negative:** nothing checks that a user's app keeps 17.0. A user who resolves the
  `project.yml` merge in favour of their old 16.0 gets a build error from the shell code, not a
  clear message.
