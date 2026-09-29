# ADR-0023: The web shell is the published library crate `mobiler-web`, which an app's web client calls in one line (`mobiler_web::run::<App>()`) and upgrades by a version bump; it is never copied into the app

Status:        Accepted
Date decided:  2026-05-26
Deciding PRs:  none (pre-PR history — commit b64c154 "Extract mobiler-web: the generic Widget→DOM web shell as a reusable crate", merged by 49916ed; first crates.io release c75a0e1, 2026-05-27)
Supersedes:    none
Code anchor:   mobiler-web/ (its own workspace; `run`, `WebApp`), demos/*/web/src/main.rs, mobiler/agentic/shared-ui.md, mobiler/src/plugin.rs (module doc: plugins are Android + iOS only)
Conformance:   .github/workflows/ci.yml job "web (mobiler-web, wasm)", .github/workflows/ci.yml job "web demo (${{ matrix.dir }})"

## 1. Context (The Problem)

ADR-0001 makes every shell a generic renderer that no app edits. It does not say how a shell
reaches an app. The Android and iOS shells are Kotlin and Swift projects. They can't be a Cargo
dependency, so `mobiler new` copies them into the app, and `mobiler upgrade` merges framework
changes back in (ADR-0012). The web shell is Rust (Leptos, compiled to WASM), so it could be either
copied like the others or depended on like any crate.

It began as a demo's hand-written web client (commit 9514a9c, fullstack-todo `web-widgets`) that
rendered the core's `Widget` tree to the DOM.

## 2. Hypothesis

If the web shell is a library crate, `mobiler-web`, whose `run::<A: WebApp>()` mounts any core
speaking the fixed ABI, then:

- an app's web client is one line of Rust plus a minimal `index.html`, with no shell code in the
  app;
- web shell fixes reach an app by a Cargo version bump, with no merge step;
- the web shell is released with the libraries (`mobiler-ui` → `mobiler-core` → `mobiler-web`,
  ADR-0009) and versioned by semver like them.

### 2.1. Refutation Conditions

- **Condition 1 — the crate builds on its own.** `mobiler-web` is a standalone WASM crate.
  - **Validation Metric:** `.github/workflows/ci.yml` job "web (mobiler-web, wasm)".
- **Condition 2 — an app's web client is the one-line call.** Each of the five web demos in the
  CI matrix is
  `fn main() { mobiler_web::run::<…::App>(); }` against the crate. If rendering moved into apps, or
  `run` stopped being the whole integration, these clients would stop building.
  - **Validation Metric:** `.github/workflows/ci.yml` job "web demo (${{ matrix.dir }})" (trunk-builds
    five such clients).

## 3. Considered Options & Rationale for Refutation

- **Option A — copy the web shell into each app, like the native shells** `[reconstructed]`
  Not chosen. Nobody recorded the comparison. The inferred cost: every app would carry thousands
  of lines of renderer, and fixes would need the three-way merge that ADR-0012 later built for
  Kotlin and Swift, which cannot be crates.
- **Option B — each app writes its own web UI and shares only the domain types** `[recorded: commit 092d715 ("the other reuse strategy (own Leptos UI, shared domain)")]`
  Built as a demo beside the shared-view one, as a strategy an app may choose. It is not the
  framework's web shell: it gives up "one UI everywhere". The `shared-ui` agentic guide says "Do
  not write a separate web UI" (mobiler/agentic/shared-ui.md).
- **Option C — a generic renderer crate that any core plugs into** `[recorded: commit b64c154 ("Apps become one line:"; "`run::<A>()` is generic over any crux App speaking the fixed ABI")]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. Commit b64c154 moved the demo's renderer into `mobiler-web` and slimmed the demo to one
line. `mobiler-web` 0.6.0 was the first crates.io release (commit c75a0e1, 2026-05-27). Every web
client since, in `demos/{coffee,todo,barbershop,fullstack-sqlx}/web` and
`demos/fullstack-todo/web-widgets`, is that one line. The web shell has been published in every
library release after `mobiler-core`, and apps take its fixes by bumping the dependency.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the web shell needs no upgrade tooling; `mobiler upgrade`'s merge applies only to
  the copied native shells.
- **Positive:** a web shell change is built by CI, on its own and in five demos, before any app
  sees it. (The crate's unit tests are not run in CI; they run locally.)
- **Negative:** an app cannot patch its web shell. A web rendering bug waits for a `mobiler-web`
  release, where the same bug on Android or iOS could be fixed in the app's copy.
- **Negative:** plugins do not reach the web. `mobiler plugin add` injects native sources at anchors
  (ADR-0011), and a crate has no anchors: mobiler/src/plugin.rs, "Android + iOS only; web degrades
  gracefully on its own." Web capabilities are exactly the ones built into `mobiler-web`.
- **Negative:** the three shells upgrade by two different mechanisms, a version bump for web and a
  three-way merge for native, so a release has to land both (ADR-0009, ADR-0012).
- **Negative:** the CLI does not scaffold a web client. `mobiler new` creates Android and iOS; the
  app author adds the web crate by hand (mobiler/agentic/shared-ui.md).
