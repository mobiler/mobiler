---
name: release-libs
description: Bump and publish the mobiler libraries (mobiler-ui → mobiler-core → mobiler-web) to crates.io in dependency order, with the irreversible-publish gate. Use when shipping a new version of the published libs.
---

# release-libs — publish the library crates

Publishes `mobiler-ui`, `mobiler-core`, `mobiler-web` to crates.io. **crates.io publishes are irreversible** — a version can never be overwritten or unpublished. Treat every gate as mandatory.

## 1. Decide the version bumps
- **Minor** bump for additive ABI/API growth (new widgets, builders, capabilities); **patch** for fixes only.
- The three libs are usually bumped together so versions stay legible, but only bump what changed.

## 2. Bump versions + inter-crate deps
- `mobiler-ui/Cargo.toml`: `version`.
- `mobiler-core/Cargo.toml`: `version` **and** its `mobiler-ui = { ..., version = "X" }` to match.
- `mobiler-web/Cargo.toml`: `version` **and** its `mobiler-core = { ..., version = "Y" }` to match.
- Refresh lockfiles: `cargo update -p mobiler-ui -p mobiler-core` at root; then in each demo + standalone-web dir (`cargo update -p ...`) — demos use bare path deps so their locks auto-track, but run it so nothing is stale. Confirm no lockfile still references the old versions.

## 3. Verify — including that EVERY demo still compiles
- `cargo test -p mobiler-core` + `cargo clippy -p mobiler-core` (root workspace).
- `mobiler-web` is a standalone wasm workspace: `cd mobiler-web && cargo check --target wasm32-unknown-unknown` (+ clippy).
- `cargo publish -p mobiler-ui --dry-run --allow-dirty` to catch packaging issues early.
- **Confirm ALL existing demos still build against the bumped libs — never publish an ABI that breaks a consumer.** Demos use bare path deps, so they see the new ABI immediately; an exhaustive `match` or renamed item will fail to compile loudly. Coverage:
  - Rust cores: `cargo test`/`clippy` each of `demos/{coffee,todo,fullstack-todo,fullstack-sqlx,barbershop}` (and their `*/mobile`, `*/web*` sub-crates).
  - Web shells: `trunk build` each web demo (`RUSTUP_TOOLCHAIN=stable` if the demo pins an android-only toolchain).
  - Native shells: the exhaustive `Widget` matches mean every iOS/Android shell must handle any new variant — rely on the CI `iOS build (...)` ×3 and `Android build (...)` matrix lanes (and `scaffold + build (template, Android)` for the template) being green on the ship-pr.
- The authoritative gate is **all CI checks green on the version-bump ship-pr** (it runs every demo lane). Do not move to step 5 until they are.

## 4. Land the version bump
- Use **ship-pr** to merge the version-bump commit. The template pins a *published* `mobiler-core` (e.g. `"0.12"`), so a version-only bump leaves the scaffold/android-demo lanes green. Get to a clean `main` before publishing.

## 5. THE IRREVERSIBLE STEP — publish in dependency order
- **Confirm with the user before the first `cargo publish`.** (AskUserQuestion: publish now, yes/wait.)
- Confirm `main` is the merged commit and CI was green.
- Publish in order, confirming each is indexed before the next:
  1. `cargo publish -p mobiler-ui` → wait, then confirm: `curl -s -A 'ua (email)' https://crates.io/api/v1/crates/mobiler-ui | jq -r .crate.max_version` shows the new version.
  2. `cargo publish -p mobiler-core` (needs ui live first) → confirm indexed.
  3. `cd mobiler-web && cargo publish` (needs core live first) → confirm indexed.
- If `cargo publish` hangs on "waiting for ... to be available", that's normal; let it finish or confirm via the API.

## 6. Hand off
- The CLI publishes separately on a tag — see **release-cli**.
- **If the release changes anything visible on screen** (new/changed widgets, theming, layouts), capture fresh screenshots and update the relevant READMEs in the same effort — don't ship a visual change with stale images. **post-release** owns the screenshot recipe + where they live.
- Run **post-release** to update `start.md` / memory / screenshots / caches.
