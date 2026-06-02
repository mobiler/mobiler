---
name: release-cli
description: Tag and publish the mobiler CLI to crates.io via the v* release workflow, with the irreversible-publish gate. Use after the CLI's version is bumped and merged to main.
---

# release-cli — publish the `mobiler` CLI

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which publishes the `mobiler` crate to crates.io via Trusted Publishing (OIDC — no stored token). **Publishes are irreversible.**

## 1. Preconditions
- `mobiler/Cargo.toml` `version` is already bumped (minor for features, patch for fixes) **and merged to `main`** via ship-pr, with CI green.
- If this CLI release also depends on newly-published libs, those must already be live on crates.io first (see **release-libs**) — the template pins published lib versions.
- Working tree clean; on `main` at the intended commit.
- **Packaging pre-check (mandatory — the CLI ships embedded assets via `include_dir!`):** run
  `cargo package -p mobiler --allow-dirty --list` and confirm every asset the binary embeds is
  present — **all of `templates/**` and every `plugins/<name>/**`** (especially any dirs added
  this release). `cargo package` has silently dropped an embedded dir before (the `templates/`
  bug), which would publish a CLI that builds but scaffolds/`plugin add`s nothing. Never tag
  until this list looks right.
- **Device-demo new capabilities (when the release adds free plugins / capabilities / UI widgets):**
  CI + a throwaway `mobiler build android` only prove things *compile* — they don't prove the
  capability actually works on a device. Before publishing a release that introduces new
  user-facing capabilities, **wire them into a demo app** (barbershop or coffee — `plugin add`
  the plugins + add UI that calls them) and **ship a TestFlight / emulator build** so they can be
  validated on real hardware (this is also where permission prompts, pickers, and sensors first
  truly run). Wiring into a demo doubles as CI compile-coverage for the plugin-in-a-real-app.
  Don't ship a capability that has never run on a device. (Heavy native UI/permission flows
  especially — location, mic, contacts, calendar.)

## 2. THE IRREVERSIBLE STEP — tag + push
- **Confirm with the user before tagging** (AskUserQuestion: tag vX.Y.Z now, yes/wait). A standing "deploy the patch" approval from the user covers this; otherwise ask.
- `git tag -a vX.Y.Z -m "<concise summary of what this release ships>"` (the tag version must equal `mobiler/Cargo.toml`'s version).
- `git push origin vX.Y.Z`

## 3. Watch the release workflow
- Find the run **after** pushing: `gh run list --workflow=release.yml -L1 --json databaseId,headBranch,status`.
- Monitor it to a terminal state (success/failure). On failure, read the job log and fix (a failed publish leaves the version unpublished — you can re-tag after fixing, but never reuse a version that DID publish).

## 4. Confirm indexed
- `curl -s -A 'ua (email)' https://crates.io/api/v1/crates/mobiler | jq -r .crate.max_version` shows the new version.

## 5. Hand off
- Run **post-release** (update `start.md` / memory; refresh screenshots if the release changed anything visible; clean caches).
