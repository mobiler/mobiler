---
name: post-release
description: After publishing to crates.io — verify the published version is installable and works on a fresh setup, then update start.md + auto-memory, refresh README screenshots for any visual change, and clean regenerable build caches. Use after release-libs / release-cli.
---

# post-release — verify the published artifact + tidy up

Run after a publish. The first step is the most important: **prove the thing we just published actually works for a brand-new user**, not just in our workspace.

## 1. Fresh-setup smoke test (always)
Local builds use path/workspace deps and our embedded templates, so they can hide packaging bugs (e.g. a template tree dropped by `cargo package`, a missing published dep). Verify against crates.io:

- **Install the published CLI from crates.io** (not the workspace build):
  `cargo install mobiler --force` (optionally `--version X.Y.Z --root <tmp>` to pin/isolate). Confirm `mobiler --version` is the just-published version.
- **Scaffold a throwaway app** in a temp dir with that installed binary: `mobiler new smoketest --package dev.test.smoke`.
- **Build it** to prove a new user gets a working app:
  - Android (toolchain present on this box — `ANDROID_HOME=~/Android/Sdk`, NDK pinned, `JAVA_HOME=~/jdk21`): `mobiler build android` → expect a full APK.
  - Web: `cd <app>/web && trunk build` (if the flavor has web).
  - The scaffold's `shared` crate must resolve the **published** `mobiler-core` (check `shared/Cargo.toml` pins the new version and `cargo check -p shared` pulls it from crates.io).
- For a **libs-only** publish (no new CLI): scaffold with the current CLI and confirm the app pins + builds against the newly-published libs; if the CLI template still pins the old version, that's expected until the next CLI release.

**Also verify `mobiler upgrade` carries an existing app forward** (the real upgrade path, not just a clean scaffold):
- Install the **previous** published CLI (`cargo install mobiler --version <prev> --root <tmp-old>`), scaffold an app with it (old shells + an old `.mobiler/base/` baseline, or none if it predates baselines), and — to mimic a real user — make a small edit to a shell file and/or leave an installed plugin in place.
- Install the **new** CLI and run `mobiler upgrade --apply` from the app root. Confirm: framework changes landed, the user edit / plugin injections survived, any conflicts are clearly reported as `*.mobiler-new` (not silently lost), and the app **still builds** (`mobiler build android` → APK).
- Also run `mobiler upgrade --apply` on a freshly-scaffolded (current) app: it must be a clean no-op (nothing to change) — a regression here means the baseline/merge is wrong.
- Clean up the temp apps + the isolated `--root` installs afterward.

If the fresh build fails, the release is effectively broken for new users — fix forward (a new patch) immediately; do not consider the release done.

## 2. Update the resume snapshot
- Edit `start.md`: published versions, the new `main` commit, what shipped this release, and the updated NEXT pointer.

## 3. Update auto-memory
- Record the new published versions and any non-obvious lesson learned (a gotcha, a procedure refinement) in the project memory; update the `MEMORY.md` index line. Don't duplicate what git/start.md already capture.

## 4. Screenshots for any visual change
- If the release changed anything rendered (new/changed widgets, theming, layouts), capture fresh screenshots and update the relevant READMEs — never ship a visual change with stale images.
- Recipe (the established one): `cd demos/<app>/web && RUSTUP_TOOLCHAIN=stable trunk build` → serve `dist/` (`python3 -m http.server`) → `google-chrome --headless --disable-gpu --no-sandbox --hide-scrollbars --window-size=430,1000 --virtual-time-budget=6000 --screenshot=out.png http://localhost:<port>/`. Headless can't click, so for sheet/overlay/picker states set the model field open in `Default` temporarily, shoot, revert.
- Commit images under each demo's `screenshots/` and reference them with repo-relative paths; keep copies in `~/mobiler-screenshots/`.

## 5. Clean regenerable caches
- The repo fills disk fast. After a shipped release, clear regenerable build artifacts: all `target/` dirs (`find . -type d -name target -prune -exec rm -rf {} +`) and `~/.gradle/caches`. Leave `~/.cargo/registry` (slower to refetch).

## 6. Finish
- Confirm `git status` is clean (the screenshot/doc updates land via **ship-pr** if they touch tracked files; `start.md` and memory are local-only).
