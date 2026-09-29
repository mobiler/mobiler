# ADR-0009: A release publishes the libraries first; template shell code that uses new ABI lands in the CLI PR only after those libraries are live on crates.io

Status:        Accepted
Date decided:  2026-05-31
Deciding PRs:  #28 → #29 (first instance); reason first stated in #102 → #103 (publish) → #104; re-affirmed by #218 → #219 after #218 went red
Supersedes:    none
Code anchor:   mobiler/templates/**, mobiler/templates/shared/Cargo.toml.tmpl (the pinned mobiler-core), .github/workflows/ci.yml (scaffold + build (template, Android))
Conformance:   .github/workflows/ci.yml job "scaffold + build (template, Android)" (scaffolds from the template and builds against the *published* core)

## 1. Context (The Problem)

The template's `shared` crate pins a **published** `mobiler-core`, because a scaffolded app has no
path dependency. CI's template lane scaffolds a fresh app and builds it. So template code using a
new ABI item fails CI until that item is on crates.io. The split was already practised from May 2026: PR #28 shipped
the theme ABI and was published, and #29 then "wires that published ABI" into the template. PR #102
later stated the reason: it left the template untouched so the lane "stays green against published
`mobiler-core 0.17`", #103 bumped the versions for publish, and #104 propagated it. PR #218 broke the pattern, went red with an
unresolved reference to the new `AppInfo` type, and the rule was written down again.

## 2. Hypothesis

If every release runs libraries first (and their publish), then a CLI PR (the template port, the core
pin bump and the CLI bump), then:

- the template lane always builds against what a real user would download;
- the CLI never ships a template referencing unpublished ABI.

### 2.1. Refutation Conditions

- **Condition 1 — the template builds against the published core.** The CI template lane, which
  resolves the pinned version from crates.io, must be green on the CLI PR.
  - **Validation Metric:** `scaffold + build (template, Android)` in `.github/workflows/ci.yml`.
    This lane covers Android only. The iOS template is covered indirectly, through the demo iOS
    shells that are kept identical to it and built by the `iOS build (…)` lanes.

## 3. Considered Options & Rationale for Refutation

- **Option A — one PR with libraries and template together** `[recorded: PR #102 body ("template untouched so the scaffold + build (template, Android) lane stays green against published mobiler-core 0.17"); PR #218 body ("The template shells follow in the CLI PR: the scaffold lane builds the template against the *published* core") and its commit 1007550 ("hold the template shells' AppInfo send for the CLI PR")]`
  Rejected: the template lane can't pass before publish.
- **Option B — point the template at a path or git dependency during development** `[reconstructed]`
  Rejected: the lane would stop testing what users actually get.
- **Option C — libraries, publish, then CLI** `[recorded: PR #29 body ("PR A (#28) shipped the Theme ABI … and published … This PR wires that published ABI"), 2026-05-31; the same shape in #42, #72/#84/#88, #102 → #103 → #104; #218 → publish → #219; #241 → publish → #242]`
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
