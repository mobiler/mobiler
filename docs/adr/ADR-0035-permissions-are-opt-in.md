# ADR-0035: A scaffold declares no permission beyond Android's `INTERNET`; every other `<uses-permission>` and iOS usage-description string is added only for a capability the app uses — by uncommenting the template's opt-in line for a built-in, or by `mobiler plugin add` for a plugin

Status:        Accepted
Date decided:  2026-05-27
Deciding PRs:  none for the first instance (direct commit 9836ecb, 2026-05-27, released in 0.8.0 by 3d5e9a8); #3 (2026-05-28) camera usage string opt-in; #18 (2026-05-30) `plugin add` inserts a plugin's permissions
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/main/AndroidManifest.xml (INTERNET only; commented VIBRATE; `mobiler:permissions` anchor), mobiler/templates/iOS/project.yml (commented `NSCameraUsageDescription`; `# mobiler:info-plist` anchor), mobiler/src/plugin.rs (`[android].permissions`, `[ios.info_plist]` → `insert_before`), mobiler/plugins/*/mobiler-plugin.toml
Conformance:   mobiler/src/plugin.rs::add_bundled_geolocation_adds_permissions_and_plist_key

## 1. Context (The Problem)

The template shell ships the built-in capabilities in every app (ADR-0002). Until 2026-05-27 it
also declared their permissions: the Android manifest carried `VIBRATE` for `cx.haptic` in every
scaffold, whether the app used haptics or not.

A declared permission is visible to the stores and the user. Android shows it on the install
page; a dangerous permission forces a runtime prompt. On iOS a usage-description string is
required before the app touches the camera, and the template's own comment records the store
side: "the App Store flags apps that ship it without using the camera"
`[recorded: mobiler/templates/iOS/project.yml]`.

Built-ins were about to grow (photo picker, camera), and a plugin system was coming. Each would
bring its own permissions.

## 2. Hypothesis

If a fresh scaffold declares only what every app needs (`INTERNET`, because HTTP is core), and
each other permission arrives with the capability that uses it, then:

- an app never declares a permission it does not use;
- a capability that needs no permission (the photo picker, intent-based camera capture on
  Android) adds nothing;
- installing a plugin is itself the opt-in, so its permissions arrive with it.

### 2.1. Refutation Conditions

- **Condition 1 — the scaffold stays clean.** The template's `AndroidManifest.xml` has no active
  `<uses-permission>` except `INTERNET`, and `project.yml` has no active `NS*UsageDescription`.
  - **Validation Metric:** review. No test reads the real template files for this; the
    `plugin.rs` tests use a synthetic `skeleton()` manifest.
- **Condition 2 — a plugin brings its own.** `mobiler plugin add` inserts the plugin's
  `[android].permissions` at `mobiler:permissions` and its `[ios.info_plist]` keys at
  `# mobiler:info-plist`.
  - **Validation Metric:** `mobiler/src/plugin.rs::add_bundled_geolocation_adds_permissions_and_plist_key`
    asserts `ACCESS_FINE_LOCATION`, `ACCESS_COARSE_LOCATION` and
    `NSLocationWhenInUseUsageDescription` land after `add`. It fails if `add` stops inserting
    them. It would not fail if the template gained a permission.
- **Condition 3 — a missing permission fails soft.** A built-in whose permission is left
  commented answers `ok: false`, not a crash (ADR-0013).
  - **Validation Metric:** review. Android haptics does this since #227; iOS camera does not
    (see §5).

## 3. Considered Options & Rationale for Refutation

- **Option A — declare every built-in capability's permission in the template** `[recorded: commit 9836ecb]`
  What the template did before. Rejected in the commit: "Built-in capabilities ship in the shell,
  but a capability's *permission* should be opt-in so an app never requests more than it uses."
- **Option B — opt-in per permission** `[recorded: commit 9836ecb; mobiler/templates/Android/app/src/main/AndroidManifest.xml]`
  Chosen. The manifest comment: "Every other permission is opt-in: a built-in capability ships in
  the shell, but you add its permission only when you use it, so an app never requests more than
  it needs." The commit names the next case: "This is also the pattern camera capture will
  follow (CAMERA is dangerous → strictly opt-in)".
- **Option C — avoid the permission altogether where the platform allows** `[recorded: PR #3 body]`
  Used alongside B, not instead of it. Camera capture on Android goes through the system camera
  app: "the app needs **no `CAMERA` permission** at all (declaring it would force a runtime
  prompt)". The photo picker is likewise permission-less (commit 4a29864).
- **Option D — the stores reject unused permissions, so leaving them in is not safe** `[reconstructed]`
  The only store rationale written down is the iOS camera comment quoted in §1. No record says
  Google Play rejects an unused `VIBRATE`; the Android rationale on record is least privilege
  ("never requests more than it needs"), not a store rule.

## 4. Decision & Rationale for Corroboration

Option B, with C where possible. Today:

- **Android template:** `INTERNET` active; `VIBRATE` commented ("Uncomment to enable the haptics
  capability (cx.haptic)"); then the `mobiler:permissions` anchor. Camera capture needs no
  `CAMERA` (a `FileProvider` shares the destination file).
- **iOS template:** `NSCameraUsageDescription` commented as "Opt-in: uncomment to enable the
  camera capability (cx.capture_photo)"; then the `# mobiler:info-plist` anchor.
- **Built-ins that need nothing:** http (beyond `INTERNET`), storage, clipboard, share, browser,
  toast, snackbar, device, dialog, datetime, photo, and the `ticker`, `system`, `appearance`
  streams.
- **Plugins:** each `mobiler-plugin.toml` lists `[android].permissions` and `[ios.info_plist]`;
  #18 made `plugin add` insert them at the anchors (ADR-0011). The geofence manifest says it
  plainly: "Adding the plugin IS the opt-in, so these are injected (uncommented) at
  mobiler:permissions" `[recorded: mobiler/plugins/geofence/mobiler-plugin.toml]`.

So there are two routes, not one: a built-in's permission is a hand edit (uncomment), a plugin's is
automatic.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a scaffold asks for nothing beyond network access; store review and install pages
  show only what the app uses.
- **Positive:** a plugin's permissions travel with it, and an app that never installs it
  (ADR-0010) never declares them.
- **Negative:** a built-in capability can be called without its permission. On Android,
  `cx.haptic` without `VIBRATE` crashed the app until #227 (2026-09-28) made it answer `ok: false`.
  On iOS, `CameraPlugin` presents `UIImagePickerController(.camera)` without checking for
  `NSCameraUsageDescription`; iOS terminates an app that opens the camera without that string.
- **Negative (bug, found while writing this record):** `insert_before` treats a key as present if
  the file text contains it anywhere, including in a comment. The template's commented
  `# NSCameraUsageDescription:` line therefore makes `mobiler plugin add scanner` (and `video`)
  skip the camera usage string: on a fresh scaffold it prints "iOS Info.plist key already present
  (skipped)" and the key stays commented. Reproduced 2026-09-30 with a scaffold from the current
  CLI. The conformance test misses it because its `skeleton()` has no commented lines. Not fixed.
- **Negative:** the rule is enforced by review only for the template. Nothing fails if someone
  re-adds an active permission to the manifest or `project.yml`.
- **Negative:** turning on a built-in is a manual edit the app owner must know about. It is noted
  in comments, not reported by the CLI, and differs from the plugin route.
