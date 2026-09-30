# ADR-0030: The `mobiler` CLI is published only by pushing a `v*` tag, which runs `release.yml` with crates.io Trusted Publishing and no stored token; the libraries are published by hand with `cargo publish`, one crate at a time

Status:        Accepted
Date decided:  2026-05-25
Deciding PRs:  none (pre-PR history — commit e006dbc "Add crates.io badge + Trusted Publishing release workflow"; first tag publish v0.2.0 the same day); hardened by 4b0a5a7 and 1f6eb8b (2026-05-27); the manual library publish written into the runbook by #45 (2026-06-01, `.claude/skills/release-libs`, `.claude/skills/release-cli`)
Supersedes:    none
Code anchor:   .github/workflows/release.yml, mobiler/Cargo.toml (`version`, checked against the tag), .claude/skills/release-cli/SKILL.md, .claude/skills/release-libs/SKILL.md, mobiler-web/Cargo.toml (standalone workspace)
Conformance:   none — a release mechanic. The `release.yml` job has no `name:` line to cite, and it runs only on a tag; the tag-matches-version step fails the run itself. Review, and the release skills' gates.

## 1. Context (The Problem)

Four crates go to crates.io: the `mobiler` CLI and three libraries (`mobiler-ui`, `mobiler-core`,
`mobiler-web`). A crates.io publish is permanent: "a version can never be overwritten or
unpublished" (`.claude/skills/release-libs/SKILL.md`).

The CLI was first published by hand (v0.1.0). The same day, 2026-05-25, commit e006dbc added
`release.yml`: on a `v*` tag it checks the tag against `mobiler/Cargo.toml`'s `version`, then
publishes `mobiler` "(OIDC, no stored token)" (commit e006dbc). v0.2.0 was the first tag-published
version.

`release.yml` runs `cargo publish -p mobiler` and nothing else. The libraries were first published
later that day (ui and core 0.3.0; `mobiler-web` from 0.6.0 on 2026-05-27), by hand, and have been ever since. crates.io's version metadata
shows a Trusted Publishing record (`trustpub_data`) on CLI versions and none on the libraries.

ADR-0009 fixes the *order* of a release (libraries live before the CLI PR). This record is about the
*mechanism*: which crate publishes how, and the constraints that follow.

## 2. Hypothesis

If the CLI publishes only from a `v*` tag through `release.yml` with Trusted Publishing, and the
libraries publish by hand in the order the release-libs runbook sets, then:

- no crates.io token for the CLI is stored in the repo or its secrets;
- a CLI version on crates.io always matches a `v*` tag (the workflow fails on a mismatch); since
  commit 1f6eb8b each tag also gets a GitHub Release. That the tag is on `main` is the runbook's
  rule (release-cli), not something the workflow checks;
- each library publish is a separate, confirmed, human step, so a library can be published (or
  patched) without cutting a CLI release, and the other way round.

### 2.1. Refutation Conditions

- **Condition 1 — the tag names the published CLI version.** A tag whose version differs from
  `mobiler/Cargo.toml` must not publish.
  - **Validation Metric:** review. The "Verify tag matches crate version" step in `release.yml`
    exits 1 on a mismatch; it runs only on a real tag push, so no test exercises it.
- **Condition 2 — no stored token for the CLI.** The publish step takes its token from
  `rust-lang/crates-io-auth-action`, not from a repository secret.
  - **Validation Metric:** review of `release.yml` (`id-token: write`,
    `CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}`).
- **Condition 3 — libraries are not published by a tag.**
  - **Validation Metric:** review. `release.yml` publishes only `-p mobiler`; the repo has only
    `v*` tags.

## 3. Considered Options & Rationale for Refutation

- **Option A — publish the CLI by hand with a local token** `[recorded: commit e006dbc]`
  What v0.1.0 did. Replaced on day one by the tag workflow, whose stated point is "no stored
  token"; the workflow header adds "no long-lived token stored in the repo".
- **Option B — a tag workflow with a crates.io API token in repository secrets** `[reconstructed]`
  The usual pre-OIDC setup. A long-lived secret that can publish; Trusted Publishing needs none.
- **Option C — publish all four crates from `release.yml`** `[reconstructed]`
  Never built, and never written down as rejected; the libraries' manual publish is how things
  stood after 0.3.0, later codified in the runbook (#45). Reasons it would not fit today, from the
  code rather than from a recorded decision:
  - the libraries version independently of the CLI and of each other (today ui 0.29, core 0.40,
    web 0.40.1, CLI 0.59; #250 shipped a web-only patch beside a CLI minor), while one `v*` tag
    names one version checked against `mobiler/Cargo.toml`;
  - `mobiler-web` is not in the root workspace ("it's a standalone workspace",
    `mobiler-web/Cargo.toml`), so `cargo publish -p` from the root can't reach it (a workflow could
    still `cd` into it; this is friction, not a blocker);
  - the runbook publishes in dependency order and confirms each crate is indexed before the next
    (`.claude/skills/release-libs/SKILL.md`, step 5). That is what ADR-0009 needs before the CLI
    PR can go green.
- **Option D — CLI on a tag via Trusted Publishing; libraries by hand, gated** `[recorded: commit e006dbc (the CLI half)]` The library half is practice, recorded as practice in `.claude/skills/release-libs/SKILL.md` ("The CLI publishes separately on a tag — see **release-cli**."); its rationale is reconstructed (Option C).
  Chosen (the library half by practice, see Option C).

## 4. Decision & Rationale for Corroboration

Option D. The workflow has published every CLI version from v0.2.0 to v0.59.0. Two follow-ups kept
it working: 4b0a5a7 pinned `crates-io-auth-action@v1.0.4` ahead of the Node 20 removal, and 1f6eb8b
added a GitHub Release per tag after "The Releases page froze at v0.2.0".

The release skills gate both paths on the maintainer: release-libs says "Confirm with the user
before the first `cargo publish`", and release-cli asks before tagging. `CLAUDE.md` states the rule
for both: "Every crates.io publish and every `v*` tag is irreversible."

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the CLI's publish credential is short-lived and issued per run; there is no CLI
  token to leak or rotate.
- **Positive:** a CLI-only patch is a version bump and a tag, with no library publish.
- **Negative:** the libraries still publish with a maintainer's local crates.io API token, so the
  long-lived credential the workflow avoids still exists for three of the four crates.
- **Negative:** two publish paths. A release can publish the CLI and forget a library, or publish
  libraries in the wrong order; only the runbook and ADR-0009's template lane catch that.
- **Negative:** Trusted Publishing is bound, on crates.io, to the repository owner and name and
  the workflow file name `release.yml` (the workflow header lists them). Renaming the file or
  moving the repo breaks CLI publishing until the crates.io setting is updated.
- **Negative:** libraries get no git tag and no GitHub Release; only crates.io records their
  versions.
- **Negative:** a pushed tag cannot be taken back once it has published. A failed run leaves the
  version unpublished and can be re-tagged, but a published version can never be reused.
