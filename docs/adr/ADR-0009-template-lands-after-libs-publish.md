# ADR-0009: A release publishes the libraries first; template shell code that uses new ABI lands in the CLI PR only after those libraries are live on crates.io

Status:        Accepted
Date decided:  2026-09-27
Deciding PRs:  #218, #219
Supersedes:    none
Code anchor:   mobiler/templates/**, mobiler/templates/shared/Cargo.toml.tmpl (the pinned mobiler-core), .github/workflows/ci.yml (scaffold + build (template, Android))
Conformance:   .github/workflows/ci.yml job "scaffold + build (template, Android)" (scaffolds from the template and builds against the *published* core)

## 1. Context (The Problem)

The template's `shared` crate pins a **published** `mobiler-core`, because a scaffolded app has no
path dependency. CI's template lane scaffolds a fresh app and builds it. So template code using a
new ABI item fails CI until that item is on crates.io. PR #218 went red exactly this way ("Unresolved
reference 'AppInfo'") when the template change rode with the library change.

## 2. Hypothesis

If every release is two PRs (libraries and demo shells first, then publish, then the template port,
core pin bump and CLI bump), then:

- the template lane always builds against what a real user would download;
- the CLI never ships a template referencing unpublished ABI.

### 2.1. Refutation Conditions

- **Condition 1 — the template builds against the published core.** The CI template lane, which
  resolves the pinned version from crates.io, must be green on the CLI PR.
  - **Validation Metric:** `scaffold + build (template, Android)` in `.github/workflows/ci.yml`.

## 3. Considered Options & Rationale for Refutation

- **Option A — one PR with libraries and template together** `[recorded: PR #218 went red; memory note "template shell lands after publish"]`
  Rejected: the template lane can't pass before publish.
- **Option B — point the template at a path or git dependency during development** `[reconstructed]`
  Rejected: the lane would stop testing what users actually get.
- **Option C — libraries, publish, then CLI** `[recorded: PRs #218 → publish → #219; every release since, e.g. #241 → publish → #242]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. The design release followed it exactly: #241 (the version bump) merged, the three libraries
were published, then #242 (the template port, the `mobiler-core = "0.40"` pin and CLI 0.58.0) went
green against the published crates.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the CLI on crates.io always scaffolds something that builds.
- **Negative:** every feature release is two PRs, with a publish gate between them.
- **Negative:** between the two, `main`'s demos use the new ABI while the template doesn't, so the
  template lags until the CLI PR. Template ports must be done carefully, as a diff from the reference
  shell (barbershop).
