# ADR-0015: Barbershop's native shells are the reference: a release builds its shell changes in `demos/barbershop` first, and the CLI templates are ported from barbershop's diff in the release's CLI PR

Status:        Accepted
Date decided:  2026-09-17
Deciding PRs:  #205 → #206 (first release run barbershop-first for both shells, per plan 2026-09-17-large-touch-targets.md); followed by #210 → #211, #213 → #214, #215 → #216, and the design release #221–#240 → #242
Supersedes:    none
Code anchor:   demos/barbershop/{Android,iOS}, mobiler/templates/{Android,iOS}, .github/workflows/ci.yml (the `iOS build (…)` lane and its "byte-identical" comment)
Conformance:   none — no check compares the template shells with barbershop's; the `iOS build (${{ matrix.app }})` lane compiles barbershop's copy, which guards the template only while the two stay identical, and review keeps them so

## 1. Context (The Problem)

The template shells can't be built and run while a feature is developed. They pin the published
`mobiler-core` (ADR-0009), CI compiles the template only for Android (`scaffold + build (template,
Android)`), and no job compiles the template's iOS shell at all. So shell work has to be written,
compiled and device-checked in a demo app, then carried to the template. Five demos each carry
their own copy of the shells, and they drift apart in package names, installed plugins and
customisations. With no fixed source, each port picked a different demo, and the template drifted
from all of them.

## 2. Hypothesis

If one demo, barbershop, always gets a release's shell changes first, and the template is ported
from barbershop's diff for that release (with the template placeholders put back), then:

- the template's iOS `Render.swift` stays byte-identical to barbershop's, so the barbershop iOS CI
  lane compiles the template's iOS code;
- each other template file's divergence from barbershop stays constant across a release, so a port
  can be checked by diffing the diffs;
- the feature is device-checked (TestFlight, AVD) in barbershop before the template ships it.

### 2.1. Refutation Conditions

- **Condition 1 — the template's iOS `Render.swift` equals barbershop's.** On 2026-09-29,
  `demos/barbershop/iOS/Sources/Render.swift` and `mobiler/templates/iOS/Sources/Render.swift` are
  byte-identical.
  - **Validation Metric:** review (`cmp` of the two files). No test checks it.
- **Condition 2 — a port leaves the per-file divergence unchanged.**
  - **Validation Metric:** review, recorded in the CLI PR (PR #242: "The template↔barbershop
    divergence is unchanged per file").

## 3. Considered Options & Rationale for Refutation

- **Option A — edit the template first and copy it to the demos** `[reconstructed]`
  Rejected in effect: the template can't use the new ABI until the libraries are published
  (ADR-0009), and it has no iOS build lane.
- **Option B — port from whichever demo the feature was built in** `[recorded: PR #42 body, "`Render.swift` ported from the CI-verified coffee shell"; PR #88 and PR #97 bodies, "byte-identical to the coffee shell" / "byte-identical to coffee"; PR #193 body, "The shell blocks were copied from `demos/saldo` rather than rewritten"]`
  The practice before barbershop became fixed. PR #84 (2026-06-02) already took Android from
  barbershop ("Android reverse-substituted from barbershop"). Replaced because the source changed
  from release to release.
- **Option C — barbershop is the reference** `[recorded: docs/superpowers/plans/2026-09-17-large-touch-targets.md, "Edit barbershop first, `cp` the iOS file to the 3 identical ones, and port by anchor to saldo iOS + the other 4 Android files."; docs/superpowers/plans/2026-09-27-theme-palette.md, "Native shell work happens in **barbershop's** shells, which are near-identical to the templates."; PR #211 body, "ported the Release 2 changes from the merged barbershop shells. Each file's +/- lines are identical to barbershop's."; PR #242 body, "get every design-release shell change from the barbershop reference shells"]`
  Chosen. Why barbershop and not coffee is not written down `[reconstructed]`: barbershop is the
  showcase that uses the most widgets, has a TestFlight lane (PR #46, like coffee), and is where each
  feature's demo usage lands.

## 4. Decision & Rationale for Corroboration

Option C. From the large-touch-targets release on, the feature-release plans and specs put shell
work in barbershop and leave `mobiler/templates/` untouched until the CLI PR (for example the
2026-09-21 shell-labels plan, and the Moj Termin design specs such as
`docs/superpowers/specs/2026-09-28-snackbar-design.md`, whose work lands as "**barbershop** shells.
Templates are ported in the release CLI PR."). The CLI PR ports
barbershop's diff and records the check: PR #206 copied `Render.swift` from barbershop
("byte-identical again, as the CI comments state") and applied the `MainActivity.kt` changes
"hunk-for-hunk"; PR #242 applied "The barbershop diffs since CLI 0.57.1" with placeholders and
placed `Core.kt` by anchor. A scaffold from the #242 CLI built a full Android APK against the
published core 0.40.0.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** every template shell change has run on a device in barbershop first.
- **Positive:** a port is mechanical and reviewable: the same hunks, with placeholders.
- **Negative:** during the design release only barbershop got the new shell code. The coffee,
  todo and fullstack-todo iOS `Render.swift` files, byte-identical to the template at PR #219
  (the last CLI release with new ABI before the design release), now differ from it. The `ci.yml` comments that
  said every demo's iOS shell equals the template were corrected with this record; only the
  barbershop lane guards the template's iOS shell.
- **Negative:** the other demos lag the template. A feature is not exercised in them unless a PR
  ports it there too.
- **Negative:** barbershop's app-specific shell edits must never leak into the template. The port
  is a hand-applied diff checked by review, not by a tool.
- **Negative:** nothing fails if a template file is edited directly and barbershop is not. The
  rule depends on review.
