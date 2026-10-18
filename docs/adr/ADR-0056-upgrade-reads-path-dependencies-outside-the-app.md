# ADR-0056: `mobiler upgrade` follows an app's `path` dependencies out of the app folder and reports the framework pins it finds there, but never writes outside the app; a duplicate framework version in `Cargo.lock` is named by `doctor` and by a failed build

Status:        Accepted
Date decided:  2026-10-18
Deciding PRs:  #300
Supersedes:    none
Code anchor:   mobiler/src/upgrade.rs (outside_manifests, path_values, workspace_root, pin_notes), mobiler/src/lockcheck.rs
Conformance:   mobiler/src/upgrade.rs::crates_outside_the_app_reached_by_path_are_reported_not_edited, mobiler/src/upgrade.rs::crates_outside_the_app_at_target_are_quiet, mobiler/src/lockcheck.rs::two_versions_name_the_crates_that_need_the_old_one, mobiler/src/lockcheck.rs::one_version_each_is_quiet, mobiler/src/lockcheck.rs::an_old_framework_crate_is_named_with_its_version

## 1. Context (The Problem)

ADR-0055's `foreign_pins` scans the manifests under the app folder. Appointments keeps its app logic
in `crates/mobile-admin`, a crate of the repository's main workspace that `shared` depends on by
path. Upgrading to CLI 0.70.0 moved `shared` to mobiler-core 0.47 and crux 0.20 but never saw that
crate, still on 0.46 / 0.18. The build had two copies of each and failed with
`MobilerShell<AdminApp>: crux_core::App is not satisfied`, an error that names no version and reads
like an API break. Only `cargo tree -i` showed the cause. Earlier upgrades needed the same hand
bump. `[recorded: appointments feedback, 2026-10-18]`

## 2. Hypothesis

If upgrade follows `path` dependencies from the app's manifests wherever they lead, it can list
every framework pin the app builds, including ones outside the app, as the hand edits ADR-0055
already prints. If the CLI also reads `Cargo.lock` for a second version of a framework crate, any
duplicate that still slips through (a git dependency, a pin upgrade can't parse) is named with the
crate that pulls it, at the moment the build fails and from `mobiler doctor`.

### 2.1. Refutation Conditions

- **Condition 1 — outside crates are reported and left alone.**
  A crate outside the app reached by path (transitively, loops included) with an old
  `mobiler-core` or a dev-dependency on an old `crux_core` gets a `(edit by hand)` note under a
  `../`-relative path. So does its workspace root when it inherits a framework dependency
  (`dep.workspace = true`). None of those files changes.
  - **Validation Metric:** `crates_outside_the_app_reached_by_path_are_reported_not_edited`.
- **Condition 2 — no false notes.** Outside crates already at the target produce nothing, and a
  `[lib] path = "src/lib.rs"` line is not taken for a dependency.
  - **Validation Metric:** `crates_outside_the_app_at_target_are_quiet`.
- **Condition 3 — the lock names the culprit.** Two versions of `crux_core` or `mobiler-core` in
  `Cargo.lock` give one message per old version, naming the app crates that require it (not the
  old framework crate that comes with them, which is named with its version when it is the only
  one). One version each, no lock, or an unreadable lock
  gives nothing.
  - **Validation Metric:** `two_versions_name_the_crates_that_need_the_old_one`, `one_version_each_is_quiet`.

## 3. Considered Options & Rationale for Refutation

- **Option A — have `--apply` bump the outside crates too** `[recorded: appointments' second ask; designer's call]`
  Those files belong to another workspace that also builds other things (appointments' holds the
  server). Writing there makes upgrade's write surface depend on where an app's path dependencies
  happen to point. ADR-0055 already chose "report, don't edit" for crates that aren't the
  template's. A one-line note was all appointments needed.
- **Option B — run `cargo metadata` / `cargo tree -d` during upgrade** `[recorded: appointments' first ask]`
  The resolved graph only reflects the new pins after cargo re-resolves, which can need the network
  and rewrites `Cargo.lock`. A review run would see the old graph. Following the files is offline
  and sees the pins as they will be.
- **Option C — only a release note** `[recorded: appointments' third ask]`
  This helps once, and only for people who read it.

## 4. Decision & Rationale for Corroboration

`foreign_pins` also checks every manifest `outside_manifests` returns: the walk starts at the app's
manifests, follows each `path = "…"` value whose directory holds a `Cargo.toml` (target paths name
files, so they drop out), and keeps each manifest once by its canonical path. For an outside
manifest that inherits `crux_core`, `mobiler-core`, `mobiler-web` or `facet` from its workspace, the
nearest ancestor manifest declaring `[workspace]` is checked too. Notes use the same targets and
caret rules as ADR-0055. Upgrade only reads these files: nothing outside the app folder is written.

`lockcheck` reads `Cargo.lock` as TOML. `mobiler doctor`, run from an app, prints one line and adds a
warning per duplicate. `mobiler dev` and `mobiler build` add the duplicates as context to a failed
host Rust build, codegen step or iOS build. Resolution and the network are never involved.

**Mutation proof:** each mutation was reverted afterwards:
- (a) dropping the `outside_manifests` loop from `foreign_pins` failed
  `crates_outside_the_app_reached_by_path_are_reported_not_edited` (no notes: `[]`);
- (b) not checking the workspace root of an inheriting crate failed the same test (two notes, the
  `_outer/Cargo.toml` one missing);
- (c) `if versions.len() < 3` in `duplicates` failed `two_versions_name_the_crates_that_need_the_old_one`
  and `versions_order_numerically` (no messages).

## 5. Consequences (Positive and Negative Predictions)

- Positive: an app whose core lives elsewhere in the repository gets the exact hand edits from
  `mobiler upgrade`, before its first failing build.
- Positive: a duplicate framework version is named in words at the failure, whatever caused it.
- Negative: upgrade now reads files outside the app folder. A `path` value pointing at an
  unreadable or huge tree costs a read per manifest, no more. A path dependency whose `path` is
  built by a macro or a `[patch]` in a config file is not followed.
- Negative: the lock check sees the lock as of the last resolve. Right after `upgrade --apply` it
  still shows the old graph until cargo runs.
