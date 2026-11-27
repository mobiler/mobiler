# ADR-0069: An image source `asset:<path>` names a file the app bundles; `image`, `avatar` and the video `poster` accept it, every shell checks the path by one rule compared per byte, code unit or scalar (never per grapheme) and resolves it under one `mobiler-assets` prefix, and the core passes the string unchanged

Status:        Accepted
Date decided:  2026-11-27
Deciding PRs:  the libs PR of the `[icon]` / `[assets]` release (mobiler-web 0.51.0); the template copies follow in the CLI PR (CLI 0.75.0, ADR-0009)
Supersedes:    none
Code anchor:   mobiler-web/src/lib.rs (`image_src`), demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt (`assetModel`, `assetPath`), demos/barbershop/iOS/Sources/Render.swift (`assetURL`, `isAssetSource`)
Conformance:   mobiler-web/src/lib.rs::non_asset_sources_pass_through, mobiler-web/src/lib.rs::asset_paths_resolve_under_mobiler_assets, mobiler-web/src/lib.rs::invalid_asset_paths_render_as_failed_load, xtask/tests/adr_conformance.rs::adr_0069_asset_resolvers_follow_one_rule

## 1. Context (The Problem)

An `image`, an `avatar` or a video `poster` took a URL (`https:`, `file:`) and nothing else. An app that wanted a
picture to show offline (an onboarding illustration, a placeholder avatar, a guide's step image) had to download it
first or hand-copy it into each native project, three times, under names each shell resolved differently.

The source is a `String` in the ABI (ADR-0001), the same string on all three shells. Whatever names a bundled file
must therefore be a string convention every shell reads identically, with no new widget field (ADR-0007), and it
must not let a path escape the app's own files: a source can come from data the app received, so `..` must never
reach outside the bundled folder on any shell.

## 2. Hypothesis

If an image source `asset:<path>` names a file the app bundles, accepted by `image`, `avatar` and the video
`poster` and by no other string field, and every shell applies the same rule to `<path>` before resolving it under
one fixed prefix, then:

- an app ships offline images with one string that means the same file on Android, iOS and the web;
- the core and the ABI are unchanged: the core passes the string as it is (ADR-0001, ADR-0007);
- no `asset:` source resolves outside the bundled folder on any shell, and a failing source looks exactly like a
  failed load (a placeholder; an avatar shows its initials).

**The rule** (identical in every shell): the source starts with `asset:`; the rest is non-empty, has no leading
`/`, no `\`, and no segment (split on `/`) that is empty, `.` or `..`. No `%` decoding: the path is used as written.
The comparison unit is a byte, a UTF-16 code unit or a Unicode scalar, **never a grapheme cluster**: Swift's
`String` joins a combining mark onto the character before it, so per grapheme `asset:../\u{0301}x` is the single
segment `../́x`, passes, and resolves one folder above the assets.

**Resolution:**

- **Web:** the relative URL `mobiler-assets/<path>`, each segment percent-encoded with the RFC 3986 unreserved
  bytes kept, so a space or a non-ASCII letter survives the URL parse.
- **Android:** `file:///android_asset/mobiler-assets/<path, encoded as on the web>` for Coil (image, avatar); the
  raw path through `AssetManager.open("mobiler-assets/<path>")` for the poster.
- **iOS:** `Bundle.main.resourceURL/<MobilerAssetsFolder>/<path>`, where Info.plist's `MobilerAssetsFolder` names
  the bundled folder (a folder reference keeps its directory's name, so the prefix is read, not fixed).

Every shell keeps the files under that one prefix, so they never collide with a plugin's or a library's own
assets. How the app's folder reaches each build (`mobiler.toml` `[assets]`) and the `[icon]` sync are separate
decisions that follow this one (ADR-0070, ADR-0071).

### 2.1. Refutation Conditions

- **Condition 1 — the web resolver follows the rule.** A valid path resolves under `mobiler-assets/`, encoded;
  an empty path, a leading `/`, a `\`, and an empty, `.` or `..` segment each give a failed load; a non-`asset:`
  source passes unchanged.
  - **Validation Metric:** `asset_tests` in `mobiler-web/src/lib.rs` (`non_asset_sources_pass_through`,
    `asset_paths_resolve_under_mobiler_assets`, `invalid_asset_paths_render_as_failed_load`).
- **Condition 2 — the native resolvers carry the same checks and prefix.** Kotlin's `assetPath` and Swift's
  `assetURL` each check `..`, `.`, empty segments, a leading `/` and `\` in live code; Android resolves under
  `mobiler-assets/`, iOS under `MobilerAssetsFolder`; Swift tests the prefix and splits per Unicode scalar.
  - **Validation Metric:** `adr_0069_asset_resolvers_follow_one_rule` in `xtask/tests/adr_conformance.rs` (the
    native code has no unit-test harness, as for ADR-0013), plus the device checks of the release.
- **Condition 3 — no ABI change.** `mobiler-ui` and `mobiler-core` are not touched by this decision.
  - **Validation Metric:** review; the release publishes mobiler-web alone among the libraries.

## 3. Considered Options & Rationale for Refutation

- **Option A — copy the images into each shell, as `[fonts]` does** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §3]`
  Rejected: every image committed four times (the app folder plus three shells), plus pruning and ownership logic
  for stale copies. Fonts needed copies for fixed names and registration; images don't.
- **Option B — a new `Widget` field or ABI type for bundled images** `[reconstructed]`
  Rejected: an ABI change (ADR-0008) for what a string convention carries; every app and shell would move for it,
  and the source field already reaches all three shells unchanged.
- **Option C — no prefix: resolve `asset:` paths at the root of each platform's assets** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §4.2]`
  Rejected: an app's file could collide with a plugin's or a library's own assets (Android's `assets/` and the iOS
  bundle root are shared), and the shells would resolve differently.
- **Option D — `asset:<path>` checked by one rule, resolved under one prefix** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §4.2, §4.3]` Chosen.

## 4. Decision & Rationale for Corroboration

Option D. The rule is written once in prose (§2) and three times in code, so the conformance test reads each
native resolver's live body for the checks, and the web resolver is unit-tested. The Swift resolver was first
written with `String.hasPrefix`, `split` and `==`, which compare grapheme clusters; review found that
`asset:../\u{0301}x` passed it while Rust and Kotlin rejected it. It now tests the prefix with
`unicodeScalars.starts(with:)` (one `isAssetSource` helper, used by every call site) and splits
`unicodeScalars` on `/`, comparing each piece as a String built from its scalars. The test bans `hasPrefix("asset:")`
and a String `split(separator: "/"` in the resolver, so the grapheme form can't come back.

Not checked here: the iOS shell is compiled by CI's barbershop lane only, and its device pass (an `asset:` image,
avatar and poster, including a space and a non-ASCII name) belongs to the release's device checks.

**Mutation proof** (2026-11-27; each mutation reverted after it was seen to fail):

- (a) Deleting `|| it == ".."` from Kotlin's `assetPath` failed `adr_0069_asset_resolvers_follow_one_rule`:
  "ADR-0069: demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt assetPath lost
  `it == ".."`".
- (b) Switching Swift back to a String split (`String(String.UnicodeScalarView(scalars)).split(separator: "/", …)`)
  failed it: "ADR-0069: demos/barbershop/iOS/Sources/Render.swift assetURL lost `scalars.split(separator: slash,
  omittingEmptySubsequences: false)`". The pre-fix `Render.swift` (grapheme version) failed it too: "assetURL lost
  `guard isAssetSource(source)`"; and putting `source.hasPrefix("asset:")` back at the image call site failed it:
  "tests `asset:` with hasPrefix (grapheme clusters); use isAssetSource".
- (c) Removing `".."` from `image_src`'s segment check failed `invalid_asset_paths_render_as_failed_load`:
  "assertion `left == right` failed: asset:../a.png / left: "mobiler-assets/../a.png" / right: """.
- (d) Changing the web prefix to `String::from("assets")` failed `adr_0069_asset_resolvers_follow_one_rule`
  ("ADR-0069: mobiler-web's image_src lost `String::from("mobiler-assets")`") and
  `asset_paths_resolve_under_mobiler_assets` (left: "assets/a.png").
- (e) Returning `String::new()` for a non-`asset:` source failed `non_asset_sources_pass_through` (left: "").

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** offline bundled images on every shell, with one string an app writes once.
- **Positive:** no ABI change; an app that never writes `asset:` sees nothing new.
- **Positive:** a path from data the app received can't reach outside the bundled folder on any shell.
- **Negative:** Android shows an empty box for a missing asset, as for any failed image; nothing tells the
  developer the name was wrong.
- **Negative:** iOS is checked only by compile in CI plus a device pass; the native checks are pinned by a
  source-level test, not by running them.
- **Negative:** names are case-sensitive on Android, iOS and most web servers; `asset:Logo.png` and `logo.png`
  differ, which a case-insensitive macOS or Windows file system can hide during development.
- **Negative:** every bundled file grows the app download, on every platform.
- **Follow-up:** the template's copies of the Android and iOS resolvers arrive in the CLI PR (ADR-0009,
  ADR-0015); that PR adds the template's `MainActivity.kt` and `Render.swift` to the path lists in
  `adr_0069_asset_resolvers_follow_one_rule`.
