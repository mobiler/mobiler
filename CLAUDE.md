# Working on mobiler

Rules for anyone changing this repo, human or agent. They are short on purpose: each one exists
because breaking it has already cost something.

## Decisions: check them before, record them after

`docs/adr/` holds the **Architecture Decision Records**: why the framework is shaped the way it
is. `docs/adr/index.md` lists every one.

- **Before designing anything**, read `docs/adr/index.md` and every record the change touches. Name
  the ones that apply in the design or spec (`Constrained by: ADR-0003, ADR-0007`). A design that
  breaks an accepted ADR must say so openly and supersede it (below). It must never just ignore it.
- **When a choice constrains future work**, write an ADR in the same pull request as the code. That
  is a choice someone could reasonably make differently, where doing it differently would break
  something structural: the wire ABI, a compatibility promise, a release mechanic, the
  framework-vs-app boundary.
- **Merged ADRs are immutable.** Only two repairs are allowed: the `Status:` line (including
  `Superseded by ADR-NNNN`) and a stale `Conformance:` path whose test moved. A decision that
  changes gets a **new** ADR that supersedes the old one, so the history stays readable.
  **If you are tempted to edit an ADR, you are writing a new one instead.**
- Not every change needs one. A bug fix, a refactor that changes no contract, or a behaviour detail
  belongs in the PR body.

See `docs/adr/README.md` for numbering, the status vocabulary, conformance tests, and the
provenance tags for rationale written from history.

## Where things are written

| Document | What it is | Lifecycle |
|---|---|---|
| `docs/adr/` | **decisions** and why | immutable once merged; superseded, never edited |
| `docs/superpowers/specs/` | a feature's approved design | written before the work; amended with dated notes |
| `docs/superpowers/plans/` | how a feature is built, task by task | written before the work |
| crate `README.md`s, `capabilities.json` | what the framework does *now* | living; updated with the code (`cargo run -p xtask -- gen-readme`) |

**Every README that describes a behaviour moves with it.** When a change alters something an app
developer can see (a CLI command or its output, an upgrade rule, a builder or ABI type, a template
file), grep every `README.md` for the area (e.g. `mobiler upgrade`, `palette`, `.mobiler-new`) and
update each match in the same PR. The root `README.md`, `mobiler/README.md` and the library READMEs
often describe the same thing. List the READMEs you checked in the PR body.

## Tests

- `cargo test --workspace` runs the root workspace (the CLI, `mobiler-ui`, `mobiler-core`, `xtask`).
  `mobiler-web` and each demo are separate workspaces, tested on their own and in CI.
- The ADR checks live in `xtask/tests/`:
  - `adr_docs.rs` keeps the ADR set well-formed (numbering, headers, status, index, and that every
    cited conformance test exists).
  - `adr_conformance.rs` asserts that decisions with a mechanical invariant still hold.
- **A conformance test written for an ADR must be proven to fail by a deliberate mutation before it
  is trusted.**
  Record the mutation in the owning ADR (a `Mutation proof` note in §4), not only in the PR. A test
  that has never been seen to fail is decoration.

## Release rules that bite

- Libraries publish first (`mobiler-ui` → `mobiler-core` → `mobiler-web`). Template code that uses
  a new ABI item lands in the **CLI** PR *after* the libraries are live (ADR-0009).
- Every crates.io publish and every `v*` tag is irreversible. Confirm with the maintainer first.
