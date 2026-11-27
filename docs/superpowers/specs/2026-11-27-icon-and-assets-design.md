# `[icon]` and `[assets]` in `mobiler.toml` — design

Date: 2026-11-27
Status: draft, awaiting maintainer approval
Requests: `docs/requests/2026-11-27-app-icon.md`, `docs/requests/2026-11-26-bundled-media.md` (temporary,
not committed; their substance is restated here)
Constrained by: ADR-0001, ADR-0007, ADR-0009, ADR-0013, ADR-0022, ADR-0023, ADR-0035, ADR-0039, ADR-0042,
ADR-0046, ADR-0048, ADR-0063, ADR-0066
New ADRs: ADR-0069 (the `asset:` scheme), ADR-0070 (`[assets]` sync), ADR-0071 (`[icon]` sync) — see the 2026-11-27 amendment

## 1. Intent

Another project building on mobiler asked for two things every real app needs before it reaches testers or
a store:

1. **Its own launcher icon**, declared once instead of hand-editing generated shell resources in three
   platforms' formats. Today an app gets the template icon, and changing it takes platform knowledge
   (Android density buckets, adaptive layers, iOS opaque 1024 PNG).
2. **Images that ship inside the app** (onboarding, guides, empty states) and render offline from
   `image(...)`, without hosting them or downloading them first.

Both follow the existing model: `mobiler.toml` is the source of truth and the CLI makes the shells match
it, as `[fonts]` (ADR-0022) and `[splash]` (ADR-0048) already do.

## 2. Goals, non-goals, anti-goals

Goals
- `[icon]` produces the Android adaptive + legacy icons, the iOS app icon and the web favicon from one
  foreground PNG and one background colour, on every `build` / `dev` / `watch`.
- `[assets] dir` makes every file under it reachable as `asset:<path>` from `image`, `avatar` and the
  video `poster`, on iOS, Android and web, offline.
- Both survive `mobiler upgrade` with no conflicts and no review copies.
- Without either section, nothing changes from today.

Non-goals
- iOS 18 dark/tinted icon variants, Android round-icon artwork distinct from the square one, a web app
  manifest (no web shell has one today).
- Image formats other than PNG as icon input. Assets may be any file the platform image loader reads.
- Resizing or compressing assets: they ship as given.
- `asset:` in any string other than image sources (video URLs, PDF view, captions) — later, if asked.

Anti-goals (a failure even if it works)
- Overwriting an icon the app made by hand (Saldo has one, tutorial chapter 10).
- Half-written icon resources after a validation error.
- Duplicating every asset into each shell's tree in git.
- A path in `asset:` that reads outside the assets folder.

## 3. Approaches considered

**Icon**
- **A. Sync like `[splash]` (chosen).** Regenerate on every run; write a file only while it is stock, carries
  the sync's header, or matches the sync's ledger; validate everything before writing anything.
- B. One-time `mobiler icon` generator. Rejected: a changed PNG would need a manual re-run, and the
  generated copy goes stale silently, against the request's acceptance criterion 2.
- C. Generate at build time, uncommitted. Rejected for the reason ADR-0022 gives: a plain Gradle, Xcode or
  trunk build would miss the files.

**Assets** (maintainer picked A on 2026-11-27)
- **A. Point each build at the app's assets folder (chosen).** One marker block per shell; no committed
  copies; edits are picked up by the next build with no sync step.
- B. Copy into each shell like fonts. Rejected: every image committed four times, plus pruning and
  ownership logic for stale copies. Fonts needed copies for fixed names and registration; images don't.

**Web support** — maintainer chose to ship it in this release (2026-11-27), which makes this a
libraries + CLI release: `mobiler-web` first, then the CLI (ADR-0009).

## 4. Design: `[assets]`

### 4.1 Config

```toml
[assets]
dir = "assets"   # relative to the app root; default "assets" when the section is present without dir
```

`dir` must be a relative path inside the app root, no `..`, no symlinked component (fsguard, as every CLI
write). A missing directory is a warning, not an error (the app may add it later); the markers are still
written so the build finds it once it exists. Android needs the folder to exist for its copy task to be a
no-op; the task must tolerate a missing source.

### 4.2 The `asset:` convention

- `asset:<relative/path>`: `/` separators, no leading `/`, no empty, `.` or `..` segment, no `\`, no `%`
  decoding (the path is used as written). A source that fails these rules renders as a failed load.
- Inside each shell the files live under one fixed prefix, **`mobiler-assets/`**, so they never collide
  with a plugin's or a library's own assets and every shell resolves the same way.
- A missing file renders exactly as a failed network load of the same widget (placeholder; an avatar
  falls back to its initials). It never crashes.
- The core is unchanged: it passes the string. No ABI change (ADR-0001/0007).

### 4.3 Per shell

| Shell | How the folder gets into the build | How `asset:x` resolves |
|---|---|---|
| Android | `mobiler:assets` marker block in `app/build.gradle.kts`: a `Sync` task copying `<dir>` into `build/generated/mobiler-assets/mobiler-assets/`, that directory added to `sourceSets.main.assets`, and `preBuild` depending on the task. The copy is build output, not committed. | `file:///android_asset/mobiler-assets/x` (each segment percent-encoded) passed to Coil for image and avatar; the poster reads `mobiler-assets/x` through `AssetManager`. |
| iOS | Two marker pairs in `project.yml`: `mobiler:assets-sources` under the target's `sources` (`- path: ../<dir>`, `type: folder`, `buildPhase: resources`) and `mobiler:assets-info` under `info.properties` (`MobilerAssetsFolder: <basename>`), because the bundle folder takes the directory's basename. (The builder checks first whether XcodeGen can fix the folder's name, which would remove the second pair.) | `Bundle.main.resourceURL/<MobilerAssetsFolder>/x`, loaded through the existing downsampling `FileImageView` (off main thread, cached). |
| Web | `mobiler:assets` marker block in `web/index.html`: `<link data-trunk rel="copy-dir" href="../<dir>" data-target-path="mobiler-assets"/>`. | `mobiler-web` maps `asset:x` to the relative URL `mobiler-assets/x` for `<img>` in image, avatar and the video poster. |

iOS prefix: the bundle folder can't be renamed by a folder reference, so instead of a fixed
`mobiler-assets/` folder iOS reads the basename from Info.plist. The visible contract (`asset:x`) is the
same everywhere.

### 4.4 Sync and lifecycle

- A new `mobiler/src/assets.rs`, run from the same hooks as fonts/splash (build, dev, watch, doctor line).
- In `build.gradle.kts` and `project.yml` (template files) it only fills the marker blocks. A file without
  the markers gets a warning naming `mobiler upgrade --apply`, and is never edited (ADR-0048's rule).
- `web/index.html` is not a template file (no web shell is scaffolded, `upgrade` never touches it), so it
  follows the fonts/splash web precedent: the block is inserted before `</head>` when missing and
  replaced afterwards. Same for the `mobiler:icon` web block.
- Removing `[assets]` empties the blocks. The app's folder is never touched.
- `mobiler watch` adds `<dir>` to its watched paths, so a changed image triggers a rebuild.
- `mobiler upgrade` never touches `<dir>` (it isn't a template path) and merges the Gradle and
  `project.yml` marker blocks like splash's.

## 5. Design: `[icon]`

### 5.1 Config

```toml
[icon]
foreground = "brand/icon-foreground.png"         # square PNG, >= 1024x1024, transparent background
background = "#f3f4f7"                            # #RRGGBB
monochrome = "brand/icon-mono.png"                # optional; Android 13+ themed icon; same size rules
padding = 18                                      # optional; % clear on each side; 0..=40; default 18
```

**Padding** is one number for every platform: the percentage of the visible icon's side kept clear on each
side of the foreground. Default 18 because a square foreground with 18% padding fits inside Android's
smallest (circle) mask, which shows only the 66 dp circle of the 72 dp visible area; the request's own
example uses 18.

Icon sources belong outside `[assets] dir` (my call): everything under `dir` ships in every build, and an
iOS folder reference can't exclude files. The example and README keep them in `brand/`; the sync warns
when a source path lies under `dir`, and doesn't filter it out.

Validation (all before any write; any failure writes nothing and names the reason): file exists, is a PNG,
is square, is at least 1024 px, at most 16384 px (splash's limit); colour is `#RRGGBB`; padding in range.
A foreground with no transparency is accepted (it just covers the background).

### 5.2 Generated files

Android (minSdk 26, so adaptive icons are always available; the legacy rasters serve launchers that
ignore them):
- `res/mipmap-anydpi/ic_launcher.xml` and `ic_launcher_round.xml`: adaptive icon with
  `@color/mobiler_icon_background`, `@mipmap/mobiler_icon_foreground`, and a `<monochrome>` layer that is
  `mobiler_icon_monochrome` when set, else the foreground layer itself, so the themed icon is
  never lost. That's what the stock template does today; Android themes by the layer's alpha. (My call.)
- `res/values/mobiler_icon.xml`: the background colour.
- `res/mipmap-{m,h,xh,xxh,xxxh}dpi/mobiler_icon_foreground.png` (and `mobiler_icon_monochrome.png`):
  the 108 dp layer (108/162/216/324/432 px) with the foreground centred in the 72 dp visible area at the
  configured padding.
- `res/mipmap-*/ic_launcher.webp` and `ic_launcher_round.webp`, rewritten **in place as WebP**
  (lossless) composited from background + foreground; the round one masked to a circle. Same file names
  as the template, so no `.png` ever sits next to a `.webp` of the same resource (Android fails that build
  with a duplicate-resource error).
- The template's `drawable/ic_launcher_foreground.xml` / `ic_launcher_background.xml` stay as they are
  (unreferenced while `[icon]` is set).

iOS: `Assets.xcassets/AppIcon.appiconset/icon-1024.png` — background + foreground composited, opaque, no
alpha channel (App Store rejects alpha). `Contents.json` unchanged (already single-size).

Web (only when `web/index.html` exists): `web/icon/mobiler-icon-32.png`, `mobiler-icon-180.png`
(apple-touch-icon), and a `mobiler:icon` marker block in `index.html` with the two `<link>`s and a trunk
`copy-dir` for `web/icon`. `web/icon/` is shared with the app the way `web/splash/` is: only the sync's
files are ever removed.

### 5.3 Ownership (the anti-goal of never clobbering a hand-made icon)

ADR-0048's rules, extended:
- `mipmap-anydpi/ic_launcher.xml` and `ic_launcher_round.xml` join `SEED_PATHS` (ADR-0042): from this
  release `mobiler upgrade` creates them when missing and never merges them. Today they are Shell-class
  and merged; Saldo's hand-edited one survives only because the template hasn't changed.
- Every file the icon sync may write is written only while it is: missing, byte-equal to the template's
  stock (CRLF-tolerant for text, as splash), marked with the sync's header (XML), or equal to the hash
  recorded in `.mobiler/icon.json` (binaries and anything without comments). Otherwise: a warning naming
  the file, and the whole icon sync for that platform is skipped (a half-branded icon is worse than none).
- Files only the sync creates (`mobiler_icon_*`, `values/mobiler_icon.xml`, `web/icon/mobiler-icon-*`)
  are written with `[icon]` and removed without it.
- **Removing `[icon]`** restores the template's stock content to every file whose current content still
  matches the ledger, removes the sync's own files and empties the web block. The ledger stays, marked
  inactive (splash's fix for remove-then-add).
- Write order as splash: new layers before the XML that references them; references cleared before the
  files they name are removed; ledger last.
- `.mobiler/icon.json` must be committed (README says so, as for `splash.json`).

### 5.4 Interaction with `[splash]`

The Android 12+ splash falls back to `@mipmap/ic_launcher`, so the declared icon also shows at launch
when `[splash]` has no logo. Nothing to change; worth a line in the README.

## 6. Release shape

1. **Libraries PR** (`mobiler-web` minor only; `mobiler-ui` / `mobiler-core` unchanged): `asset:` mapping
   for image, avatar and video poster, plus tests. Publish `mobiler-web`. Barbershop (the reference shell,
   ADR-0015) gets the native Android/iOS renderer changes in this PR too, so the CLI PR ports from its diff.
2. **CLI PR**: `assets.rs`, `icon.rs`, the template marker blocks and renderer changes, `SEED_PATHS`
   additions, the hooks, ADR-0070 + ADR-0071 with conformance tests (ADR-0069 lands in the libs PR) and
   mutation proofs, `mobiler upgrade --apply` in every CLI-managed demo (ADR-0063), READMEs + site +
   Saldo tutorial.
3. **Saldo adopts `[icon]`** in the CLI PR (maintainer: yes, 2026-11-27, since we can redraw it). The
   "S" monogram is redrawn as a 1024 px transparent foreground (white DejaVu Sans Bold "S", sized so the
   composite at padding 18 matches today's `icon-1024.png` to within 2 px) and committed as
   `demos/saldo/brand/icon-foreground.png`; `background = "#0e9f8e"`; no `monochrome` (the white S is its
   own themed layer). Its hand-made Android files are reset on purpose before the first sync:
   `mipmap-anydpi/ic_launcher*.xml` and `drawable/ic_launcher_background.xml` back to the template's,
   `drawable-xxxhdpi/saldo_launcher_foreground.png` deleted; then the sync regenerates everything. Tutorial
   chapter 10 §4 shrinks to the config.
   The `asset:` scheme gets a small card in barbershop (the kitchen-sink demo), not in Saldo.

## 7. Conformance (ADR tests, each proven by a mutation)

- ADR-0071: the icon markers and seed paths ship in the template; a hand-edited icon file is left alone
  with a warning; a bad PNG writes nothing; removing `[icon]` restores stock; no `.png` and `.webp`
  share a resource name after a sync.
- ADR-0070 (and ADR-0069 for the resolvers): the template's `mobiler:assets` marker pairs ship (one in `build.gradle.kts`, two in
  `project.yml`); an `asset:` path with `..`, a leading `/` or a
  `\` is rejected by each resolver (web unit test; Kotlin/Swift by a source-level conformance check that
  the validator exists, as earlier ADRs did); `upgrade` never writes under `<dir>`.

## 8. Verification

- Unit tests in `icon.rs` / `assets.rs` mirroring splash's.
- Android emulator (API 26 and 36): adaptive icon, themed icon on 13+, legacy raster on a launcher that
  shows it; `asset:` PNG and JPEG in image, avatar, poster; missing asset shows the placeholder.
- iOS simulator build + the maintainer's iPhone check (home-screen icon, `asset:` images).
- Web: trunk build of a demo, headless check that `mobiler-assets/x` loads.
- `mobiler upgrade` on a CLI-managed demo with both sections reports nothing pending.

## 9. Left to the builder

- Exact Kotlin/Swift/Rust shape of the shared `asset:` validator and resolver.
- Lossless WebP via the `image` crate's `webp` feature (or another pure-Rust encoder if that feature
  doesn't encode); keep the CLI's dependency growth minimal.
- Verify first that trunk's `copy-dir` accepts `../<dir>` with `data-target-path`. If it doesn't, stop and
  ask: every fallback (a symlink, a copy) changes approach A.
- Wording of warnings and README text.

## Amendment 2026-11-27 (during the libs PR)

- The `asset:` contract (scheme, rule, `mobiler-assets/` prefix, per-shell resolution) is fixed irreversibly by
  the `mobiler-web` 0.51.0 publish, so its ADR ships in the libs PR as **ADR-0069**. The `[assets]` sync becomes
  **ADR-0070** and the `[icon]` sync **ADR-0071**, both in the CLI PR.
- Validators compare per byte, code unit or Unicode scalar, never per grapheme (a Swift `split` on a String
  let `../` followed by a combining mark through as one segment).
- Android percent-encodes each path segment for Coil, as the web does, so names with `#`, `?` or `%` load on every shell.
- Barbershop uses `[assets] dir = "assets/bundled"`: an app's `assets/` often already holds font and splash
  sources, which the default folder would bundle a second time. The CLI warns when a `[fonts]`, `[splash]` or
  `[icon]` source lies under `dir` (generalising §5.1's icon-only warning).
- `MOBILER_WEB_PIN` moves to 0.51 in the libs PR (ADR-0066's conformance test requires it with the bump).
