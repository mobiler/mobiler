# iOS glass tabs, new icons, Saldo finished — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 8 icons and an iOS-only floating glass tab bar option to the framework, then ship Saldo as a CLI-managed
app that keeps its look.

**Architecture:** Libraries PR (mobiler-ui `Icon` variants + `Scaffold.ios_glass_tabs`, mobiler-core builder,
mobiler-web glyphs, every demo shell updated, barbershop renders glass) → libraries publish → CLI PR (template ported
from barbershop, Saldo's held branch rebased and finished) → device checks → CLI 0.72.0.

**Tech Stack:** Rust (facet/serde ABI types, xtask conformance), SwiftUI (iOS 17 target, iOS 26 `glassEffect`),
Kotlin/Compose (material-icons-extended), Leptos (mobiler-web).

**Spec:** `docs/superpowers/specs/2026-10-19-ios-glass-tabs-and-icons-design.md`

## Global Constraints

- Versions: `mobiler-ui` 0.33.0, `mobiler-core` 0.48.0, `mobiler-web` 0.48.0, CLI 0.72.0.
- New `Icon` variants, in this order, appended after `DoneAll`: `Ledger`, `Chart`, `Wallet`, `CreditCard`, `Lock`,
  `Download`, `Upload`, `Globe`.
- New `Widget::Scaffold` field `ios_glass_tabs: bool`, appended after `bottom_bar`; builder
  `mobiler_core::with_ios_glass_tabs(widget: Widget) -> Widget` (sets it on a `Scaffold`, passes anything else through),
  and every existing scaffold builder keeps the field (like `with_bottom_bar`, #239).
- Doc text on the field and builder: "iOS only: a floating Liquid Glass tab bar (iOS 26; a frosted capsule below) and a
  glass FAB. Android and the web keep the standard bar."
- Unset option and unused icons render exactly as before (ADR-0019 spirit).
- Moj Termin is not touched. Coffee, todo, fullstack-todo get only what they need to compile.
- Libraries publish and CLI tag are irreversible: each needs the maintainer's OK first.
- Cargo target dir on data2; Android builds with `JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64`.
- Phone: check `mCurrentFocus` before any `adb input`; never press Back twice; stay-awake on only while driving it.
- Commit messages via `git commit -F`.

## Review Focus

1. **Lost Saldo behaviour** (third time this would bite): Task 1's audit must be by commit and content.
2. **Glass bar overlap:** the last row of a body, a `bottom_bar`, the snackbar and the FAB must stay reachable above the
   floating capsule; kept tabs (ADR-0058) still swap under it.
3. **Old shell, new core:** the new field and icons are wire changes; every in-repo shell must decode them (CI builds
   every demo).
4. **Palette under glass:** a set `fab` / `on_fab` must still show (tinted glass / coloured glyph).
5. **ADR-0060 test strictness:** a hand-written plugin registration above an anchor must fail it.

---

### Task 1: Saldo audit (before any code)

**Files:** report only: `<workspace>/saldo-audit.md`.

- [ ] **Step 1:** `git log --oneline -- demos/saldo` on `main`. For every commit that touched a Saldo shell file
  (`demos/saldo/Android/**`, `demos/saldo/iOS/**`), read its diff and list each behaviour it added.
- [ ] **Step 2:** For each, find where the new world covers it: the CLI 0.71 template (`mobiler/templates`), a bundled
  plugin, or Saldo's Rust core; or mark it **gap**. Use `git log -S<symbol>` and `grep` in the template, not file
  lists.
- [ ] **Step 3:** Gaps that the spec already covers (glass bar #171/#173/#175, icons #180, palette #187, picker
  #183–#186) are marked so. Any other gap: if it needs an ABI item, add it to Task 2's scope and ledger it; if not
  worth a feature, **stop and ask the maintainer** before Task 2.
- [ ] **Step 4:** Ledger `Task 1: complete (N commits, G gaps: …)`.

## Libraries PR — branch `ios-glass-icons`

### Task 2: ABI — icons and the scaffold field

**Files:** `mobiler-ui/src/lib.rs`, `mobiler-core/src/lib.rs`, `mobiler-ui/Cargo.toml`, `mobiler-core/Cargo.toml`,
`mobiler-web/Cargo.toml`, root `Cargo.lock`.

**Interfaces:**
- Produces: the 8 `Icon` variants; `Widget::Scaffold { …, bottom_bar, ios_glass_tabs: bool }`;
  `mobiler_core::with_ios_glass_tabs(Widget) -> Widget`.

- [ ] **Step 1: Failing tests** in `mobiler-core`: `with_ios_glass_tabs_sets_and_every_scaffold_builder_keeps_it`
  (mirror `with_bottom_bar_sets_and_every_scaffold_builder_keeps_it`: set the flag, apply every other scaffold builder,
  the flag stays true; a non-scaffold passes through unchanged; default `scaffold(...)` has it false). In
  `mobiler-ui`: a round-trip of a `Scaffold` with `ios_glass_tabs: true` and of each new `Icon`.
- [ ] **Step 2:** Run them; expected: compile failure / missing items.
- [ ] **Step 3:** Implement: append the variants and the field (doc text from Global Constraints, "BREAKING: appended —
  full `Scaffold` literals and patterns need it", as on `bottom_bar`); the builder; every place in `mobiler-core` that
  builds or rebuilds a `Scaffold` carries the field. Bump the three crate versions and the inter-crate pins.
- [ ] **Step 4:** `cargo test -p mobiler-ui -p mobiler-core`: pass. Commit `feat(ui,core): 8 icons and the iOS-only
  glass tabs option on Scaffold`.

### Task 3: mobiler-web

**Files:** `mobiler-web/src/lib.rs`.

- [ ] **Step 1:** A test that `icon_glyph` returns a non-empty glyph for each new icon (fails to compile until arms
  exist); the scaffold match binds the new field as ignored (`ios_glass_tabs: _`).
- [ ] **Step 2:** Glyphs: Ledger "🧾", Chart "📈", Wallet "👛", CreditCard "💳", Lock "🔒", Download "⬇", Upload "⬆",
  Globe "🌐" (builder may adjust to match the existing glyph style).
- [ ] **Step 3:** `cargo test` in `mobiler-web` (its own workspace): pass. Commit.

### Task 4: Every demo shell decodes and draws the new items

**Files:** each demo's `Android/app/src/main/java/**/MainActivity.kt` icon `when`, `iOS/Sources/Render.swift`
`sfSymbol` switch, and every `.scaffold(...)` pattern in `iOS/Sources/{Core,Render}.swift` (coffee, todo,
fullstack-todo/mobile, saldo, barbershop).

- [ ] **Step 1:** Regenerate a demo's bindings and build (`cargo build -p shared --features uniffi` + codegen) to see
  the exhaustiveness failures. Expected: Kotlin `when` / Swift `switch` non-exhaustive errors.
- [ ] **Step 2:** Add the arms. Android (material-icons-extended): Ledger `Icons.AutoMirrored.Filled.ReceiptLong`,
  Chart `Icons.AutoMirrored.Filled.ShowChart`, Wallet `Icons.Default.AccountBalanceWallet`, CreditCard
  `Icons.Default.CreditCard`, Lock `Icons.Default.Lock`, Download `Icons.Default.Download`, Upload
  `Icons.Default.Upload`, Globe `Icons.Default.Language`. iOS: `list.bullet.rectangle.fill`,
  `chart.line.uptrend.xyaxis`, `wallet.bifold.fill`, `creditcard.fill`, `lock.fill`, `arrow.down.circle`,
  `arrow.up.circle`, `globe`. Scaffold patterns get one more `_`.
- [ ] **Step 3:** Build barbershop's Android locally (assembleDebug); CI builds the rest. Commit
  `chore(demos): every shell draws the new icons and decodes the new scaffold field`.

### Task 5: Barbershop renders glass tabs (iOS) + Profile toggle

**Files:** `demos/barbershop/iOS/Sources/Render.swift` (ScaffoldView), `demos/barbershop/iOS/Sources/Core.swift`
(pattern), `demos/barbershop/app-core/src/lib.rs` (toggle).

- [ ] **Step 1: Failing test** (barbershop app-core): toggling "Glass tabs (iOS)" in Profile sets the root scaffold's
  `ios_glass_tabs`; default false.
- [ ] **Step 2:** Implement the toggle (beside "Large controls"); test passes.
- [ ] **Step 3: iOS rendering** (from Saldo's old `glassPill` / `frostedPill` / `floatingTabBar`, `git show
  335ce45:demos/saldo/iOS/Sources/Render.swift`):
  - phone layout with tabs and the flag → the tab buttons (`tabButtons`, with badges and `tabSpoken`) in a floating
    capsule overlaid at the bottom; the body gets bottom padding so its end scrolls clear; no classic bar;
  - FAB → glass circle (plain) or glass capsule with label (extended); palette `fab` tints the glass
    (`glassEffect(.regular.tint(...))` under `#if compiler(>=6.2)` + `#available(iOS 26.0, *)`, frosted below),
    `on_fab` colours glyph/label, else brand colour; raised above the capsule;
  - `bottom_bar` stays pinned above the capsule; snackbar lifted above both;
  - rail (regular width) unchanged; flag false → byte-identical rendering to before.
- [ ] **Step 4:** Commit `feat(ios): glass tabs option in barbershop (ADR-0059)`.

### Task 6: ADR-0059 + conformance + docs

**Files:** `docs/adr/ADR-0059-platform-only-options.md`, `docs/adr/index.md`, `xtask/tests/adr_conformance.rs`,
READMEs (`mobiler-ui/README.md` icon / scaffold docs, root `README.md`, `mobiler/README.md`: grep `bottom_bar`,
`with_bottom_bar`, `Icon`), `capabilities.json` if it lists scaffold options (then `cargo run -p xtask -- gen-readme`).

- [ ] **Step 1: Failing test** `adr_0059_platform_only_option_is_named_and_ignored_elsewhere`: the field name starts
  with `ios_`; its doc in `mobiler-ui` contains "iOS only"; barbershop's and (later) the template's `Render.swift`
  read `iosGlassTabs`; no Android `MainActivity.kt` and no `mobiler-web/src` code reads `iosGlassTabs` /
  `ios_glass_tabs` except an ignore binding (`ios_glass_tabs: _`). Run: fails until the template has it — scope the
  template path in Task 8 (ledger a ruling if the test is split).
- [ ] **Step 2:** ADR-0059 (decision: a shared-core option only one platform renders is named for that platform and
  ignored elsewhere; first use `ios_glass_tabs`; considered: a `Theme` field (breaks literal construction), an Android
  floating toolbar (Material's is for actions), hiding the option per platform (impossible in one shared core)).
  Mutation proof: (a) read the flag in barbershop's `MainActivity.kt`; (b) drop "iOS only" from the doc; (c) remove
  the iOS read. Record each failure message.
- [ ] **Step 3:** READMEs and the BREAKING note for full `Scaffold` literals/patterns. Commit.

### Task 7: Ship the libraries

- [ ] **Step 1:** Fresh code + security review (opus) with Review Focus 2–4; one fix pass, test-first.
- [ ] **Step 2:** `ship-pr`; all CI green (every demo's Android/iOS lanes). Dispatch TestFlight (barbershop); the
  maintainer checks the Profile toggle on iPhone (both bars, FAB, badges, kept tabs, bottom bar).
- [ ] **Step 3:** `release-libs` (ui → core → web), **with the maintainer's OK**; demo lockfiles follow if the skill
  says so.

## CLI PR — branch `saldo-cli-managed` (rebased)

### Task 8: Template ported, CLI 0.72.0

**Files:** `mobiler/templates/iOS/Sources/{Render,Core}.swift`, `mobiler/templates/Android/.../MainActivity.kt`,
`mobiler/templates/shared/Cargo.toml.tmpl` and root `Cargo.toml.tmpl` pins (core 0.48), `mobiler/Cargo.toml` 0.72.0.

- [ ] **Step 1:** Rebase `saldo-cli-managed` onto `main` (after Task 7). Resolve conflicts in favour of main for
  libraries/demos, keeping the branch's Saldo work.
- [ ] **Step 2:** Port barbershop's shell diff from the libraries PR to the template (drift count unchanged, as in
  #303). Run the ADR-0059 test over the template too: pass.
- [ ] **Step 3:** Bump pins + CLI version. `cargo test --workspace`: pass. Commit.

### Task 9: Saldo on the 0.72 template, with its look

**Files:** `demos/saldo/**` (via `mobiler upgrade --apply`), `demos/saldo/shared/src/app.rs`.

- [ ] **Step 1:** `mobiler upgrade --apply` in `demos/saldo` with the branch's CLI; expected: shells move, nothing left
  pending; `mobiler upgrade` again: "Up to date".
- [ ] **Step 2:** Saldo's Rust: tabs use `Icon::Ledger` / `Icon::Chart` / `Icon::Wallet`; root scaffold
  `with_ios_glass_tabs`; `Theme.palette` restoring #187 (light background `#E9F6F1`, brand teal for selection/primary
  text so no system blue; dark as before) — values from `git show ff3ccdb` and `7dbfb5e`. Saldo's Rust tests pass.
- [ ] **Step 3:** Each Task 1 audit item is now covered; list them for the PR body. Commit.

### Task 10: ADR-0060 test tightened (review I4, I5, M1, M3)

**Files:** `xtask/tests/adr_conformance.rs`, `docs/adr/ADR-0060-saldo-is-cli-managed.md` (unmerged: edit in place),
`mobiler/src/new.rs` test, `demos/saldo/Android/build-android.sh` mode.

- [ ] **Step 1: Failing tests:**
  - `plugin_insertions_only` takes the allowed lines (from every bundled `mobiler-plugin.toml` Saldo uses: its
    `register` lines rendered, gradle deps, permissions, plist lines) and rejects an inserted line not among them —
    unit test: a hand-written `"foo" to FooPlugin(),` above the anchor → Err;
  - every Saldo plugin source equals the bundled source rendered;
  - every template file is in Saldo's base or on an explicit allowlist (Own/Seed classes, with a reason each);
  - `Info.plist` and `settings.gradle.kts` compared against the template;
  - the display-name exception matches exactly `CFBundleDisplayName: "Saldo"`, once;
  - `new_app_scripts_are_executable` asserts the three script paths exist in `mobiler/templates`.
- [ ] **Step 2:** Watch each fail (mutate where the code already passes), implement, pass. `git update-index
  --chmod=+x demos/saldo/Android/build-android.sh`.
- [ ] **Step 3:** ADR-0060 text and mutation proof updated (add: a hand-written registration above an anchor). Commit.

### Task 11: Picker, workflow, amendment

**Files:** `mobiler/plugins/picker/{android,ios}/*`, Saldo's copies (via `mobiler plugin add`/upgrade drift),
`.github/workflows/android-apk-saldo.yml`, `docs/superpowers/specs/2026-10-19-ios-kept-tabs-and-saldo-refresh-design.md`.

- [ ] **Step 1:** Picker: Android resolves on any dismiss (`setOnDismissListener` → `ok:false "cancel"` if not
  resolved); iOS: present only from a VC not `isBeingDismissed`, walk to the top presented VC, and answer `ok:false
  "could not present"` if the sheet isn't presented after `present`'s completion; both default `selected` to -1 and
  treat out-of-range as none. Update Saldo's copies to equal the bundled ones (Task 10's test enforces it).
- [ ] **Step 2:** Workflow: header comment states the per-run key (update = uninstall = data loss unless exported);
  build-tools pick excludes `-rc` (`ls ... | grep -v rc | sort -V | tail -1`).
- [ ] **Step 3:** Dated amendment on the 2026-10-19 kept-tabs spec: the hold, the framework-first decision, this
  spec. Commit.

### Task 12: Device checks, ship, release

- [ ] **Step 1:** Build Saldo's APK; A32 (stay-awake on while driving): glass option is a no-op on Android; new tab
  icons; Aqua Mint background; picker (language, currency); biometric (ask the maintainer to touch); export; kept
  tabs; relaunch persistence. Uninstall/reinstall allowed (no data to keep).
- [ ] **Step 2:** Fresh code + security review (Review Focus 1, 2, 5); one fix pass.
- [ ] **Step 3:** `ship-pr`; merge on green. Dispatch TestFlight (saldo); the maintainer checks the glass bar, FAB,
  icons, colours, picker sheet on iPhone.
- [ ] **Step 4:** After the maintainer's OK: `release-cli` 0.72.0, `post-release` (smoke install, upgrade from 0.71),
  stay-awake off, `pstree` check, memory + `start.md`.
