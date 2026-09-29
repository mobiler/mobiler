# ADR-0022: Custom fonts are files declared in the app-root `mobiler.toml` `[fonts]` and copied into every shell by `mobiler fonts sync`; the ABI carries only a family reference, never font bytes

Status:        Accepted
Date decided:  2026-09-28
Deciding PRs:  #225; #226 (2026-09-28: `mobiler watch` font paths, the `mobiler-fonts.css` name, licences)
Supersedes:    none
Code anchor:   mobiler/src/fonts.rs (Manifest, FontsSection, RoleSpec, sync, sync_for_build, watch_paths, doctor_line), mobiler/src/build.rs + dev.rs (sync at the start of a run), mobiler/src/watch.rs (font paths watched), mobiler-ui/src/lib.rs (FontFamily::Custom, FamilyRole)
Conformance:   mobiler/src/fonts.rs::sync_writes_every_shell, mobiler/src/fonts.rs::sync_is_idempotent, mobiler/src/fonts.rs::bad_files_are_skipped_with_warnings

## 1. Context (The Problem)

`Theme.font` was a closed enum of system fonts (`System`, `Rounded`, `Serif`, `Monospace`). The
Moj Termin design needed two bundled families, Space Grotesk for display text and Roboto for body
text, on Android, iOS and the web. Each platform registers a bundled font differently:
`res/font/` resources on Android, `UIAppFonts` in the Info.plist on iOS, `@font-face` on the web.
The shells are generic and CLI-owned (ADR-0001), so the app should not hand-edit them to add a
font. The CLI had no app-level config file yet.

## 2. Hypothesis

If the app lists its font files once, per role, in `mobiler.toml` `[fonts]`, and the CLI copies
them into each shell under fixed names and writes each platform's registration, then:

- one declaration serves all three shells;
- the core only says which role to use (`FontFamily::Custom`, and `FamilyRole` in a type scale), so
  no font data crosses the core↔shell boundary;
- the synced copies are ordinary committed files, so a plain Gradle, Xcode or trunk build works;
- a bad or missing font file never breaks a build: that role falls back to the system font.

### 2.1. Refutation Conditions

- **Condition 1 — one declaration reaches every shell.** A sync writes the Android resources, the
  iOS files plus the `project.yml` block (`UIAppFonts`, `MobilerFontDisplay`, `MobilerFontBody`),
  and the web files plus the `index.html` block.
  - **Validation Metric:** `sync_writes_every_shell` in `mobiler/src/fonts.rs`.
- **Condition 2 — running it again changes nothing.** Build and dev run the sync every time, so it
  must be idempotent.
  - **Validation Metric:** `sync_is_idempotent` in `mobiler/src/fonts.rs`.
- **Condition 3 — a bad file is a warning, not a failure.**
  - **Validation Metric:** `bad_files_are_skipped_with_warnings` in `mobiler/src/fonts.rs` (a
    woff2 and a truncated file give two warnings, and the good file is still synced).
- **Condition 4 — the ABI stays a reference.** `FontFamily::Custom` is a unit variant; no widget or
  theme field carries font bytes or paths. Checked by review.

## 3. Considered Options & Rationale for Refutation

- **Option A — font bytes or URLs in the ABI, registered by the shell at runtime** `[reconstructed]`
  It would need runtime font registration on every platform and would carry binary data across the
  boundary for something that never changes after build.
- **Option B — the app adds the fonts to each shell by hand** `[reconstructed]`
  Three different registrations to get right, in shell files the CLI owns and `mobiler upgrade`
  merges (ADR-0012).
- **Option C — generate the shell copies at build time only, not committed** `[reconstructed]`
  A plain Gradle, Xcode or trunk build would then miss the fonts. The spec chose committed copies
  for that reason (below).
- **Option D — `mobiler.toml` `[fonts]` as the source of truth, synced into committed shell files** `[recorded: docs/superpowers/specs/2026-09-28-custom-fonts-design.md, decisions 1–4 ("`mobiler.toml` `[fonts]` is the source of truth."; "The synced copies are ordinary committed files, so a plain Gradle, Xcode or trunk build works too."; "The sync never fails the build."); PR #225]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D. `mobiler.toml` is a new app-root file; today it holds only `[fonts]`, with a `display` and
a `body` role, each a list of files, an optional family name and an optional licence file. The CLI reads each file's weight
(OS/2 `usWeightClass`) and family from the file itself, and accepts TTF and OTF.

`mobiler fonts sync` writes, per shell:

- **Android:** `app/src/main/res/font/mobiler_<role>_<weight>.<ext>`. The shell looks them up by name
  at runtime, "so there is no compile-time dependency on the files existing" (spec).
- **iOS:** `iOS/Sources/Fonts/mobiler-<role>-<weight>.<ext>`, plus a CLI-owned block between
  `# mobiler:fonts-begin` / `# mobiler:fonts-end` in `project.yml`.
- **Web** (only when `web/index.html` exists): `web/fonts/` with `mobiler-fonts.css`, plus a
  CLI-owned block in `index.html`.

`mobiler build` and `mobiler dev` call `sync_for_build` at the start of each run. `mobiler watch`
watches `mobiler.toml` and the font source paths, so a change triggers a rebuild that re-syncs.
`mobiler doctor` reports the `[fonts]` state. With `FontFamily::Custom`, the display role covers
titles, subtitles, the top-bar title and sheet titles, and the body role covers the rest; a role
without synced files falls back to the system font. Barbershop ships this way (PR #225).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app declares its fonts once; the shells stay generic and no font data crosses
  the ABI.
- **Positive:** the committed copies make the shells buildable by their native tools alone.
- **Negative:** each font is stored several times in the app repo: the source file plus one copy
  per shell.
- **Negative:** a plain native build after editing `[fonts]` uses stale copies until the sync runs
  again (through `mobiler build`/`dev`/`watch` or `mobiler fonts sync`).
- **Negative:** only two roles exist, display and body. A third family, italics or a per-widget
  font needs a new role and new shell code. Variable fonts are accepted as files, but their weight
  axes are not instanced.
- **Negative:** `mobiler.toml` is now an app-root file the CLI owns the format of. Anything added to
  it later becomes part of the app-facing contract.
