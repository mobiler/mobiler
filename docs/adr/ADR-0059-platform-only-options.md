# ADR-0059: An option in the shared core that only one platform can render is named for that platform, documented as a no-op elsewhere, read by that platform's shell and ignored by every other; `Scaffold.ios_glass_tabs` is the first

Status:        Accepted
Date decided:  2026-10-19
Deciding PRs:  #304
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Widget::Scaffold.ios_glass_tabs), mobiler-core/src/lib.rs (with_ios_glass_tabs), demos/barbershop/iOS/Sources/Render.swift (ScaffoldView: glassPill, frostedPill, tabButtons), mobiler-web/src/lib.rs (the ignore binding)
Conformance:   xtask/tests/adr_conformance.rs::adr_0059_platform_only_option_is_named_and_ignored_elsewhere

## 1. Context (The Problem)

Saldo, the showcase app, drew a floating Liquid Glass tab bar and a glass add button on iOS (#171, #173, #175) by
editing its own iOS shell. ADR-0060 (on the held Saldo branch) makes Saldo's shells equal the template, so the look has to become a framework
option. Liquid Glass is an iOS idiom: Material 3 has no navigation-bar equivalent (its floating toolbar is for
actions), and the web has none either.

An app's core is shared by every platform, so an option can't be offered to one platform only. The maintainer asked
for it to be iOS-only and, if it does nothing elsewhere, for that to be plain to an Android developer (2026-10-19:
"it is iOS option, and if Android has something similar we should show them that, or even maybe hide option for them
in case that it doesn't do anything").

## 2. Hypothesis

If an option only one platform can render is named for that platform (`ios_…`), its doc says what the other platforms
do, and the other shells ignore it, then an app can set it once for everyone. Each platform then shows its native
style, and no developer expects it to change Android or the web.

### 2.1. Refutation Conditions

- **Condition 1: the name and the doc say which platform.** The field starts with `ios_`, and its doc says "iOS only".
  - **Validation Metric:** `adr_0059_platform_only_option_is_named_and_ignored_elsewhere`.
- **Condition 2: the iOS shell renders it, and no other shell reads it.** The reference iOS shell (and, after the
  libraries publish, the template's) reads `iosGlassTabs`. No Android shell does. The web shell binds it only as
  ignored (`ios_glass_tabs: _`).
  - **Validation Metric:** the same test.
- **Condition 3: unset changes nothing.** With the option off, every shell renders exactly as before.
  - **Validation Metric:** barbershop's Profile switch, off by default, on TestFlight and on the A32.

## 3. Considered Options & Rationale for Refutation

- **Option A: a `Theme` field** `[recorded: docs/superpowers/specs/2026-10-19-ios-glass-tabs-and-icons-design.md]`
  Rejected. `Theme` has public fields, and apps (barbershop, and likely Moj Termin) build it as a full struct literal,
  so a new field would break their code. A `Scaffold` builder doesn't.
- **Option B: a cross-platform "floating tab bar" with an Android look** `[recorded: the design spec, Goal 1]`
  Rejected. Material 3's floating toolbar holds actions, not navigation, so it would invent a non-native Android bar.
- **Option C: hide the option on other platforms** `[recorded: the maintainer's ask, 2026-10-19]`
  Impossible in one shared core: the option exists wherever the core does. Naming and documenting it is the closest
  honest form.

## 4. Decision & Rationale for Corroboration

Option: platform-named options, ignored elsewhere. The first is `Widget::Scaffold.ios_glass_tabs: bool`, set with
`mobiler_core::with_ios_glass_tabs(root)` and kept by every scaffold builder. On iOS, the phone tab bar floats as a
capsule over the body: Liquid Glass on iOS 26, and a frosted material below that (and on SDKs before Xcode 26). The
body, a bottom bar, the FAB and the snackbar stay clear of it. The FAB is glass too: a circle, or a capsule keeping an
extended FAB's label. A palette's `fab` tints the glass, and `on_fab` colours the glyph. On the iPad rail and on every
other platform nothing changes.

**Mutation proof:** the invariant was broken four times on purpose. The test failed each time; each file was restored,
and the test passed again.
- Barbershop's `MainActivity.kt` read `iosGlassTabs` (`ADR-0059: …/MainActivity.kt reads the iOS-only option`).
- The field's "iOS only" doc line was removed (`ADR-0059: ios_glass_tabs' doc must say it is iOS only`).
- Barbershop's `Render.swift` stopped reading it (`ADR-0059: … must render ios_glass_tabs`).
- The web shell bound it as a used variable (`ADR-0059: the web shell reads the iOS-only option`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** Saldo's iOS look becomes a framework feature any app can turn on, and Saldo keeps it while its shells
  equal the template.
- **Positive:** a later platform-only option has a pattern to follow.
- **Negative:** the field is a wire change: an app's shells must be upgraded with the core (as for every `Scaffold`
  field), and a full `Scaffold` literal or exhaustive pattern needs the new field.
- **Negative:** the option sits in every platform's API, so its scope rests on its name and doc rather than the
  compiler.
