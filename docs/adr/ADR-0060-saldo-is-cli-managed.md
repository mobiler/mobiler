# ADR-0060: Saldo, the showcase app, is CLI-managed and equals the current template; every CLI PR that changes a template shell runs `mobiler upgrade --apply` in `demos/saldo`, and a conformance test fails until it does

Status:        Superseded by ADR-0063
Date decided:  2026-10-19
Deciding PRs:  #305
Supersedes:    none
Code anchor:   demos/saldo/.mobiler/ (version stamp + base snapshot), .claude/skills/release-cli/SKILL.md (the upgrade step)
Conformance:   xtask/tests/adr_conformance.rs::adr_0063_every_demo_but_barbershop_is_cli_managed

## 1. Context (The Problem)

Saldo (`demos/saldo`) is the app the maintainer will invite people to try (2026-10-19: "our marketing/demo app — main
one which we will like to publish live as example people to try, so I wish that we update its code fully to match
latest"). Its native shells were a hand copy from an older CLI: about 1,200 lines behind barbershop's Android shell and
1,000 behind its iOS shell, missing the theme fix, kept tabs (ADR-0057, ADR-0058), the keyboard work and the launch
window. It had no `.mobiler/` version stamp or base snapshot, so `mobiler upgrade` couldn't refresh it, and nothing
noticed it falling behind.

Barbershop doesn't have this problem in the same way: it is the reference shell that template changes start from
(ADR-0015), and the template is ported from it.

## 2. Hypothesis

If Saldo is an ordinary CLI app (shells from `mobiler new`, plugins from `mobiler plugin add`, `.mobiler/` committed)
and a test compares it with the current template, then Saldo can't lag the template by more than one PR. It is also kept
current by the same `mobiler upgrade --apply` every app developer runs, so the showcase exercises the upgrade path.

### 2.1. Refutation Conditions

- **Condition 1: Saldo's base snapshot is the current template.** Every file in `demos/saldo/.mobiler/base/` equals its
  template rendered with Saldo's names, and the version stamp equals the CLI's version.
  - **Validation Metric:** `adr_0060_saldo_equals_the_current_template` ("Saldo's base is behind the template").
- **Condition 2: Saldo's shell code is not edited by hand.** Each of its shell code files equals the rendered template
  apart from the blocks `mobiler plugin add` inserts directly above a `mobiler:` comment anchor. Every inserted line
  must equal one of a used bundled plugin's rendered insertions. The one other exception is the iOS display name the CLI writes,
  exactly `CFBundleDisplayName: "Saldo"`, once. Each plugin source equals the bundled one, and a `…Plugin` or
  `…Activity` file from neither the template nor a bundled plugin fails. `Info.plist` and `settings.gradle.kts` equal
  the template. Its own resources (`res/`, the icon, `.gitignore`) may differ; upgrade's 3-way merge keeps those.
- **Condition 2b: nothing the template ships is missing.** Every template file is in Saldo's base snapshot, unless it
  is app-owned (upgrade's Own and Seed classes, listed in the test's `app_owned`).
  - **Validation Metric:** the same test ("is edited by hand"), with `plugin_insertions_only` unit-tested by
    `plugin_insertions_only_accepts_anchor_blocks_only`.
- **Condition 3: upgrade agrees.** `mobiler upgrade` in `demos/saldo` reports "Up to date" right after the refresh.
  - **Validation Metric:** the deciding PR's log.

## 3. Considered Options & Rationale for Refutation

- **Option A: refresh Saldo by hand once** `[recorded: the design spec, B.1]`
  Rejected. That is how it fell behind: nothing would notice the next drift.
- **Option B: compare Saldo with its own base snapshot only** `[recorded: the spec's builder review, 2026-10-19]`
  Rejected. It catches a hand edit but not a forgotten upgrade: after a template change, Saldo and its old base still
  match each other.
- **Option C: make every demo CLI-managed** `[reconstructed]`
  Deferred. Coffee, todo and fullstack-todo are smaller examples the maintainer chose not to refresh now; barbershop is
  the reference the template is ported from (ADR-0015), so it can't be generated from the template.

## 4. Decision & Rationale for Corroboration

Option: Saldo is CLI-managed and strictly compared with the current template (Conditions 1 and 2). The CLI release
skill (`release-cli`) says to run `mobiler upgrade --apply` in `demos/saldo` in any CLI PR that changes a template
shell, and to commit the result.

Saldo's own choices stay its own: the app icon (its PNG foreground renamed `saldo_launcher_foreground`, so the
template's vector foreground can never replace it), the icon background colour, `refs/` and `tmp/` in `.gitignore`,
its Rust core and its Cargo manifests (path dependencies on the workspace libraries, as for every in-repo demo).

**Mutation proof:** the invariant was broken on purpose; the test failed each time, the file was restored, and the
test passed again. First round:
- A comment line was added to Saldo's `MainActivity.kt` away from an anchor
  (`ADR-0060: demos/saldo/Android/app/src/main/java/rs/mobiler/saldo/MainActivity.kt is edited by hand …`).
- One line of the template's `Render.swift` was changed without upgrading Saldo
  (`ADR-0060: Saldo's base is behind the template (iOS/Sources/Render.swift) …`).
- `demos/saldo/.mobiler/version` was deleted (`ADR-0060: demos/saldo/.mobiler/version is missing`).

After the review pass (the first version accepted any line above an anchor, never read plugin sources and never
noticed a template file missing from the base), six more breaks each failed it:
- `"foo" to FooPlugin(),` was written above Core.kt's plugin anchor (`… Core.kt is edited by hand (line 689: …`). The
  first version of the test passed this one.
- A `FooPlugin.kt` was added to Saldo's package (`… FooPlugin.kt is not from the template or a bundled plugin`).
- A comment was appended to Saldo's `SqlitePlugin.kt` (`… SqlitePlugin.kt differs from the bundled plugin's …`).
- `PhotoPipeline.kt` was removed from the base
  (`ADR-0060: the template has …/PhotoPipeline.kt, Saldo's base doesn't: run mobiler upgrade --apply …`).
- The display name became "Saldo Pro" (`ADR-0060: Saldo's project.yml must name it exactly once: …`).
- A key was added to `Info.plist` (`ADR-0060: demos/saldo/iOS/Sources/Info.plist is edited by hand`).

The second review then found that an inserted line only had to share a word with a plugin manifest (`false`, from
`exported="false"`), and that any line mentioning `mobiler:` counted as an anchor. Now an inserted line must equal one
of a used plugin's rendered insertions, above a comment anchor. `case "x": return PluginResponse(ok: false, …)`
written above Core.swift's plugin anchor passed before and fails now (`… Core.swift is edited by hand (line 276: …`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the showcase app always runs the current shells, and the upgrade path is exercised on a real app in
  every shell-changing CLI PR.
- **Positive:** the refresh found three gaps, fixed in the deciding PR: `mobiler new` left `build-android.sh` and
  `build-ios.sh` non-executable; the template's `.gitignore` didn't ignore the iOS build output; and Saldo's
  Language and Currency pickers ran on a single-choice picker written into its old `Core.kt` / `Core.swift` by hand
  (#183). That picker is now the bundled `picker` plugin, added to Saldo with `mobiler plugin add picker`, as the
  maintainer chose (2026-10-19).
- **Negative:** every CLI PR that changes a template shell must also commit Saldo's upgrade, or CI fails.
- **Negative:** Saldo can no longer carry a shell experiment of its own. A behaviour it needs goes into the template (or
  a plugin) first.
- **Negative:** Saldo's release APK is now unsigned from Gradle. The APK workflow signs it with a throwaway debug key.
