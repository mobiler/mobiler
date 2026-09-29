# ADR-0011: `mobiler plugin add` installs a plugin by copying its native sources into the app and inserting manifest-declared lines above `mobiler:*` anchor comments; `mobiler upgrade` preserves those lines but never rewrites plugin bodies, and reports a drifted plugin instead

Status:        Accepted
Date decided:  2026-05-30
Deciding PRs:  #18 (first instance: `plugin add`, anchors, `mobiler-plugin.toml`); #33 (2026-05-31) and #44 (2026-06-01) keep anchor files mergeable on upgrade; #201 (2026-07-21) adds the drift report; #203 (2026-09-16) keys it on registration
Supersedes:    none
Code anchor:   mobiler/src/plugin.rs (Manifest, add_at, insert_before, line_has_marker, drifted, registered), mobiler/src/upgrade.rs (ANCHORS, classify, Report.plugins), mobiler/plugins/*/mobiler-plugin.toml, the `mobiler:*` comments in mobiler/templates/**
Conformance:   mobiler/src/plugin.rs::add_bundled_battery_copies_and_registers, mobiler/src/plugin.rs::templates_carry_every_anchor, mobiler/src/plugin.rs::drifted_reports_only_installed_and_stale_plugins, mobiler/src/plugin.rs::drifted_does_not_report_an_alternative_plugin_sharing_file_names

## 1. Context (The Problem)

A capability is an opaque `{plugin, op, input}` call answered by a native handler that the shell
looks up by name (ADR-0002). Adding one to an app means dropping native files into the Android and
iOS projects and then editing several shell files: the registry in `Core.kt` and `Core.swift`,
permissions in `AndroidManifest.xml`, gradle dependencies, Info.plist keys and SwiftPM packages in
`project.yml`. Done by hand, this is slow and error-prone. The mechanism also had to work for
plugins that are not bundled with the CLI (licensed packages on disk), and it had to survive
`mobiler upgrade`.

## 2. Hypothesis

If a plugin is a self-describing directory (`mobiler-plugin.toml` plus native sources), and the
templates carry fixed `mobiler:*` comment anchors, then `plugin add` can install any plugin
mechanically:

- copy `[android].sources` into the app's package dir and `[ios].sources` into `iOS/Sources`,
  substituting `{{PACKAGE}}`;
- insert each declared item (`register`, `register_stream`, `permissions`, `gradle_deps`,
  `gradle_plugins`, `manifest_application`, `app_launch`, `info_plist`, `entitlements`,
  `spm_packages`) at its anchor, skipping one already present. Most go in as one line directly
  above the anchor, with its indent (`insert_before`); array Info.plist keys, entitlements and
  SwiftPM packages have small dedicated inserters keyed on the same anchors.

Because the injected lines sit in files `upgrade` merges (ADR-0012), they
survive upgrades. Plugin bodies are the app's copies: `upgrade` never rewrites them, and it names
each installed bundled plugin whose shipped sources differ from the app's copy.

### 2.1. Refutation Conditions

- **Condition 1 — install is copy plus anchored insert.** A bundled plugin's source lands with the
  package substituted, and its registration lands in both `Core.kt` and `Core.swift`.
  - **Validation Metric:** `add_bundled_battery_copies_and_registers` in `mobiler/src/plugin.rs`.
- **Condition 2 — the real templates carry the anchors `plugin add` targets.** Otherwise the
  install only prints a warning (`anchor … not found — add it manually`) on a real app.
  - **Validation Metric:** `templates_carry_every_anchor` in `mobiler/src/plugin.rs`. It matches
    by substring, so it cannot tell `// mobiler:plugins` from `// mobiler:plugins-stream` in
    `Core.swift`: losing either one of those two still passes. Review covers that gap.
- **Condition 3 — a stale plugin body is reported.** Not installed → silent; just installed →
  silent; stale body → reported; `plugin add` again → cleared. A plugin sharing file names with
  another (`push` / `push-firebase-only`) is not reported by mistake. That `upgrade` never
  overwrites the body is checked by review: `upgrade` has no code path that writes plugin
  sources.
  - **Validation Metric:** `drifted_reports_only_installed_and_stale_plugins` in
    `mobiler/src/plugin.rs`, and `drifted_does_not_report_an_alternative_plugin_sharing_file_names`
    for the second case. That injected lines survive an upgrade is covered by the upgrade
    record's tests and by `merge_anchors_updates_shell_and_preserves_injections` in
    `mobiler/src/upgrade.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — manual install (copy files, hand-edit the registries)** `[recorded: PR #18 body, "Turns installing a plugin from a manual chore (drop native files + hand-edit `Core.kt`/`Core.swift` + manifest + project.yml) into **one command**"]`
  Rejected: that was the state before #18.
- **Option B — anchor-based injection from a manifest** `[recorded: PR #18 body]`
  Chosen. #18 calls it "the anchor for the licensed-plugin story (open-core + paid service +
  licensed add-ons)": the same `mobiler-plugin.toml` format serves bundled samples and local paid
  packages.
- **Option C — parse and rewrite the shell sources (Kotlin, Swift, XML, YAML) structurally** `[reconstructed]`
  Rejected in effect: four languages to parse, and the comment anchor plus line insert has covered
  every plugin so far. The one exception is array-valued Info.plist keys, which
  `merge_plist_array` unions instead of inserting.
- **Option D — let `upgrade` overwrite plugin bodies with the shipped version** `[recorded: PR #201 body, "Making `upgrade` auto-resync plugin bodies. That's the better long-term answer but needs real design around detecting user edits (merge vs sidecar, as template files get) — not something to rush into this tag."]`
  Deferred, not rejected. Commit 4d371f4 reports drift "rather than changing the conservative
  never-overwrite policy".
- **Option E — say nothing about stale plugin bodies** `[recorded: PR #201 body]`
  The state before #201. Rejected once it bit: after a 0.49 → 0.50 upgrade the app's
  `TransferPlugin` ignored the new `multipart` field, and "**Both halves compile**, so nothing
  catches the mismatch."

## 4. Decision & Rationale for Corroboration

Option B for install, Option D deferred, and a drift report instead of Option E. The mechanism has
held for 31 bundled plugins and local packages. Manifest fields were added as plugins needed them
(`gradle_deps`, `manifest_application`, `register_stream`, `app_launch`, `gradle_plugins`,
`spm_packages`), each with a new anchor in the templates, and never needed a structural rewrite.

The drift check (`plugin::drifted`) compares each bundled plugin's substituted sources with the
app's copies and prints `mobiler plugin add <name>` as the fix. A drifted plugin also suppresses
"Up to date. ✓". PR #203 tightened "installed" to "registered on every platform it declares",
because `push` and `push-firebase-only` share file names and file presence reported the wrong one.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** one command installs a plugin, bundled or local, and re-running it is a no-op for
  the injected lines.
- **Positive:** a new plugin needs no CLI code, only a manifest and sources, unless it needs a new
  kind of injection.
- **Negative:** the anchors are a contract. Removing or renaming one in a template or an app
  silently breaks installs (`MarkerMissing` only prints a warning). `mobiler upgrade` also carries
  a hard-coded `ANCHORS` list that must stay in step with the templates.
- **Negative:** a plugin's shell half can lag its Rust API after an upgrade. The drift report makes
  it visible but does not fix it.
- **Negative:** the fix, `plugin add` again, overwrites the app's copies of that plugin's sources
  with no merge. Local edits to a plugin body are lost unless the user backs them up first.
- **Negative:** drift is only checked for bundled plugins. A local or licensed package gets no
  report.
- **Negative:** idempotency is a substring check on the inserted line. Changing a plugin's
  `register` line in a new release adds the new line next to the old one instead of replacing it.
