# ADR-0051: An app turns its custom-scheme deep-link registration off with `mobiler.toml` `[deep_links] enabled = false`, which the CLI applies by commenting out the lines between `mobiler:deep-links` markers in the Android manifest and `project.yml`; it never writes the scheme itself, and without the section it touches nothing

Status:        Accepted
Date decided:  2026-10-14
Deciding PRs:  #292
Supersedes:    none
Code anchor:   mobiler/src/deep_links.rs (sync, toggle, sync_for_build, run_sync_cli), mobiler/src/build.rs + dev.rs (the hooks; watch rebuilds through dev's pipeline), mobiler/templates/Android/app/src/main/AndroidManifest.xml and mobiler/templates/iOS/project.yml (the marker blocks)
Conformance:   xtask/tests/adr_conformance.rs::adr_0051_deep_link_registration_sits_between_markers, mobiler/src/deep_links.rs::off_comments_out_both_registrations_and_on_restores_them_exactly, mobiler/src/deep_links.rs::a_hand_set_scheme_survives_off_and_on, mobiler/src/deep_links.rs::no_section_touches_nothing, mobiler/src/deep_links.rs::missing_markers_refuse_and_edit_nothing, mobiler/src/deep_links.rs::a_broken_block_or_config_writes_nothing, mobiler/src/deep_links.rs::the_apps_own_comments_in_the_block_are_kept, mobiler/src/deep_links.rs::a_line_that_would_stay_live_writes_nothing, mobiler/src/deep_links.rs::a_scheme_registered_outside_the_markers_is_reported

## 1. Context (The Problem)

ADR-0034 put a custom-scheme registration in every scaffold: a VIEW/BROWSABLE intent filter on
Android's main activity, and `CFBundleURLTypes` in iOS's `project.yml`, both with the bundle id as the
scheme. An app that never routes deep links still carried it, so any web page or app could start the
app's activity for no purpose.

The Moj Termin (appointments) admin app's audit raised it as MOB-18, and the team asked for a switch
rather than patching generated files `[recorded: appointments request, 2026-10-13, quoted here]`:
"a `mobiler.toml` (or `plugin`) switch such as `deep_links = false` that leaves the filter out on
Android and the URL type out on iOS." Their check: "with the switch off,
`adb shell am start -a android.intent.action.VIEW -d com.appointments.admin://x` no longer resolves to
the app."

The lines live in framework files that `mobiler upgrade` three-way merges (ADR-0046). An app could
already delete them by hand and keep that across upgrades. But nothing documented it, the two
platforms had to be edited separately, and turning it back on meant knowing what to restore.

## 2. Hypothesis

If the template wraps both registrations in `mobiler:deep-links` markers, and the CLI, on every
`build` / `dev` / `watch` and on `mobiler deep-links sync`, comments out the lines between them for
`enabled = false` and uncomments them for `enabled = true`, then:
- one line of config turns deep links off on Android and iOS together, and back on again;
- the default stays on, so no existing app or ADR-0034 behaviour changes;
- the CLI needs no knowledge of the scheme, so an app's hand-set scheme survives a round trip;
- an app without the section is never edited, and "off" is never reported as applied when it wasn't.

The rules:
- A switched-off line is its own indent, then `<!-- off: ` … ` -->` (XML) or `# off: ` (YAML). The `off:`
  tag is what `enabled = true` removes, so the app's own comments between the markers are never touched.
  Inside the block, the `off:` tag is therefore reserved.
- Blank lines and lines that are already comments are left as they are.
- **All or nothing.** Every file is checked before any is written. Nothing is written, and the sync
  reports `failed`, for any of these:
  - a bad section (`deny_unknown_fields`);
  - a file that exists but can't be read;
  - unbalanced markers, or a marker line holding anything besides the marker;
  - an XML line holding `--` (which can't sit inside a comment);
  - a block that "off" would leave with something live, such as markup after a comment on one line;
  - with `enabled = false`, a file without the markers (an app made before CLI 0.67). The warning names
    `mobiler upgrade --apply`. A half state, iOS off and Android on, is never written.
- `mobiler build` / `dev` / `watch` only warn. `mobiler deep-links sync` exits non-zero on `failed`, so a
  CI step can gate on it.
- With `enabled = false`, a registration still live outside the markers (any `android:scheme`, such as
  the `oauth` plugin's redirect filter, or a `CFBundleURLTypes`) is reported as a warning: the switch can't vouch for lines it doesn't own.
- Writes go through `fsguard`, so they never follow a symlink.

### 2.1. Refutation Conditions

- **Condition 1: the markers ship around exactly the registration.** Both templates and barbershop
  wrap the BROWSABLE filter and `CFBundleURLTypes` in one marker pair; the launcher filter is outside.
  - **Validation Metric:** `adr_0051_deep_link_registration_sits_between_markers`.
- **Condition 2: off and on are exact inverses.** Off comments both out; on restores the files byte for
  byte, a hand-set scheme and the app's own comments included.
  - **Validation Metric:** `off_comments_out_both_registrations_and_on_restores_them_exactly`,
    `a_hand_set_scheme_survives_off_and_on`, `the_apps_own_comments_in_the_block_are_kept`.
- **Condition 3: nothing without the section, nothing on bad input, and never a half state.**
  - **Validation Metric:** `no_section_touches_nothing`, `missing_markers_refuse_and_edit_nothing`,
    `a_broken_block_or_config_writes_nothing`, `a_line_that_would_stay_live_writes_nothing`.
- **Condition 4: a registration the switch doesn't own is reported.**
  - **Validation Metric:** `a_scheme_registered_outside_the_markers_is_reported`.
- **Condition 5 (runtime):** with `enabled = false`, the appointments check above no longer resolves to
  the app on an Android emulator, and with `enabled = true` it does again. Checked in PR #292.

## 3. Considered Options & Rationale for Refutation

- **Option A: document deleting the lines by hand** `[reconstructed]`
  Upgrade's three-way merge already keeps such an edit. Rejected as the answer: two files on two
  platforms, nothing to tell a reader why the lines are gone, and re-enabling means knowing the exact
  lines and the scheme. The appointments team asked for a switch instead.
- **Option B: a top-level `deep_links = false` key** `[recorded: appointments request, 2026-10-13, quoted here]`
  "a `mobiler.toml` (or `plugin`) switch such as `deep_links = false`". Rejected for a section: in TOML
  a top-level key written below `[splash]` or `[fonts]` silently becomes part of that table. A section
  also leaves room for a later `scheme`.
- **Option C: delete the lines between the markers, and regenerate them on** `[reconstructed]`
  Rejected. Regenerating needs the scheme. Android's comes from `applicationId` and iOS's from the bundle
  id, which can differ (they do in the appointments app). And a scheme the app changed by hand would be
  lost on the way back.
- **Option D: an installable plugin that adds the registration** `[reconstructed]`
  Rejected. It would turn deep links off for every existing app on upgrade (the template would lose the
  lines), a breaking change to ADR-0034's default. A plugin also only inserts lines at anchors
  (ADR-0011) and can't remove them.
- **Option E: also make the shell ignore intent data when off** `[reconstructed]`
  Rejected for now. `MainActivity` would need a generated flag, for a gap only an app naming the
  activity can reach (§5). Nobody asked for it.
- **Option F: the section, with marker blocks commented out and in** `[recorded: maintainer, 2026-10-14, quoted here]`
  Presented in chat as the design and approved: "yes". Chosen.

## 4. Decision & Rationale for Corroboration

Option F. It doesn't supersede ADR-0034: the `system` stream, its events and its default registration
are unchanged; an app can now opt out of the registration. It follows ADR-0048's shape (a `mobiler.toml`
section, synced by the CLI into marker blocks in framework files, nothing inserted into a file without
markers). It is stricter in one way: `mobiler deep-links sync` fails, rather than only warning, when the
switch can't be applied, because a switch that closes an entry point must not look applied when it
wasn't and ADR-0010 (opt-in, the default unchanged for everyone else).

Barbershop, the reference shell (ADR-0015), carries the same markers, so CI builds a marked manifest
and runs a marked `project.yml` through xcodegen. The oauth plugin's own redirect filter
(`OAuthActivity`) is outside the markers and is left on: that plugin needs it. The sync reports it (§5).

**Mutation proof:**
- Removing the template manifest's begin marker failed `adr_0051_deep_link_registration_sits_between_markers`
  ("needs exactly one").
- Moving the iOS end marker above `CFBundleURLTypes` failed it ("has a URL type outside the markers").
- Never uncommenting on `enabled = true` failed `off_comments_out_both_registrations_and_on_restores_them_exactly`.
- Commenting out the app's own comment lines failed `the_apps_own_comments_in_the_block_are_kept`. (A first
  version of that test passed under this mutation, because its XML note held `--` and the sync refused
  the file; it now asserts the switched-off state of both files.)
- Writing each file as soon as it was computed failed `a_broken_block_or_config_writes_nothing`.
- Dropping the missing-markers warning failed `missing_markers_refuse_and_edit_nothing`.
- From the review fix pass:
  - warning on missing markers but still writing the other file failed `missing_markers_refuse_and_edit_nothing`;
  - skipping the check that "off" leaves nothing live failed `a_line_that_would_stay_live_writes_nothing`
    (markup after a comment);
  - matching marker lines by `contains` only failed it too (markup on the end-marker line);
  - skipping an unreadable file failed it too (a manifest that isn't UTF-8);
  - dropping the outside-registration warning failed `a_scheme_registered_outside_the_markers_is_reported`.
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app that doesn't use deep links, and doesn't install `oauth`, can't be opened by a link
  or another app through its scheme, from one config line on both platforms, and can turn it back on the
  same way.
- **Positive:** no default changes, no ABI change, CLI-only.
- **Negative:** with the `oauth` plugin installed, Android stays openable by the same scheme through
  `OAuthActivity`'s redirect filter. The sync warns; closing it would mean moving oauth to a scheme of its
  own, a separate decision.
- **Negative:** Android's `MainActivity` still forwards the data of any intent aimed at it, so an app
  that names the activity explicitly can still send it a URL. The core sees it only if it subscribes to
  `system`.
- **Negative:** an app made before CLI 0.67 needs `mobiler upgrade --apply` for the markers before the
  switch does anything; until then `build` / `dev` warn and `mobiler deep-links sync` fails.
- **Negative:** the switch applies only when the CLI runs. A build straight from Android Studio, Xcode or
  the `build-*.sh` scripts uses the files as last synced, so the synced files must be committed; on iOS
  the change reaches `Info.plist` only when `xcodegen generate` runs.
- **Negative:** while the block is off, a framework change to those lines in a later release meets the
  app's commented copy, and upgrade leaves a conflict for review. An app with no `.mobiler/base/`
  baseline gets the template's lines back on upgrade, until the next sync comments them out again.
- **Negative:** the `off:` lines are visible in the app's files and diffs.
