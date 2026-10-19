# iOS kept tabs, demo theme fix, and a fully current Saldo

**Asked by:** the maintainer, 2026-10-19, after the Android tab-switch fix (#302, ADR-0057) merged and before CLI 0.71.0 is
tagged: "lets first do iOS and our demo apps". For iOS: "just that they behave same". For Saldo: "keep Saldo as our
marketing/demo app — main one which we will like to publish live as example people to try, so I wish that we update its
code fully to match latest". Store publishing is out of this round, "except testflight and my android attached".
**Release:** both parts ship in the still-unreleased **CLI 0.71.0** (already bumped by #302; no further bump). There is
no library, Rust or ABI change.
**Constrained by:**
- ADR-0001: the shell stays a generic renderer of the `Widget` tree.
- ADR-0005: the core's view drives every frame; a kept screen is never shown with a stale tree.
- ADR-0011: plugins are added by `mobiler plugin add`; upgrade preserves anchor lines and never rewrites plugin bodies.
- ADR-0015: barbershop's shell changes first; the template is ported from its diff in the same PR.
- ADR-0021: iOS 17 APIs are available without availability checks.
- ADR-0026: live native views keep their declarative lifecycle (never kept hidden).
- ADR-0032: the shell holds no navigation state; kept screens are a render cache keyed by what the core sent.
- ADR-0042 / ADR-0046 / ADR-0055 / ADR-0056: how `mobiler upgrade` treats seed files, review copies and framework pins
  (Part B relies on all of them working on Saldo).
- ADR-0047: children keep identity keys.
- ADR-0057: the Android rules that iOS now matches.

**New records:**
- **ADR-0058**: the iOS shell keeps visited tab screens by the same rules as ADR-0057. ADR-0057 stays in force for
  Android and is not edited.
- **ADR-0059** (my call): Saldo is the showcase app and is CLI-managed. Every CLI release that changes a shell runs
  `mobiler upgrade --apply` on Saldo in its CLI PR, so Saldo never lags the template again. This is a release
  mechanic, so it is a decision, not a PR detail.

## Why

- On Android, tabs now keep their screen and scroll position. On iOS a tab switch rebuilds the screen, resets the
  scroll to the top and cross-fades. The maintainer wants both platforms to behave the same. iOS speed was never the
  complaint; parity is.
- Coffee, todo and fullstack-todo still have the Android theme bug fixed in #302: `Theme.kt` builds a new
  `ColorScheme` on every core update, so every update (typing, a toast) recomposes the whole app.
- Saldo is the app people will be invited to try, but its shells are an old hand copy (about 1,200 lines behind
  barbershop's Android shell, 1,000 behind iOS). It has no `mobiler.toml` and no `.mobiler/base/`, so `mobiler upgrade`
  can't keep it current. That is why it fell behind.

## Goals

- **iOS**: returning to a visited tab shows the screen exactly where it was left: scroll position, an open
  `Disclosure`, a pushed detail screen the core still shows. A tab switch is instant, with no fade. This holds in
  barbershop, the template, and Saldo.
- **Android demos**: coffee, todo and fullstack-todo get the theme fix. They have no tabs, so nothing else changes.
- **Saldo**: its Android and iOS shells equal what `mobiler new` + `mobiler plugin add` produce today, plus only what
  is genuinely Saldo's own. Its `.mobiler/` state is committed, so `mobiler upgrade` reports "up to date" on it right
  after the refresh.
- Saldo is checked on the maintainer's Android phone (Samsung A32, attached by USB; I drive it) and on their iPhone
  through the TestFlight (saldo) lane.

## Non-goals

- Publishing Saldo to the Play Store or App Store (no Play lane, no listing). TestFlight is allowed, being a test
  build.
- The web shell. It would need a `mobiler-web` release; ADR-0057's note that web should follow stays open.
- A full shell refresh of coffee, todo or fullstack-todo. They get the one-file theme fix only.
- Changes to Saldo's Rust core beyond what the refresh forces. Its features and design stay as they are.
- iOS performance work. If the kept tabs cost visible time on a device, that is a finding to report, not a goal.

## Anti-goals

- A hidden iOS tab that can be tapped, read by VoiceOver, holds the keyboard, or sends an `Action`.
- A kept iOS screen shown with old content or old colours after the core changed it (for example dark mode switched in
  Saldo's Settings tab, then back to Bills).
- A Saldo whose shell "matches" by hand edits that `mobiler upgrade` would then flag as conflicts, or that loses a
  Saldo-specific behaviour (biometric lock, file export, file picker, SQLite) in the refresh.

## Order and device checks (decided by the maintainer, 2026-10-19)

- Each PR merges once CI is green. The maintainer then checks on iPhone through TestFlight: barbershop after PR 1,
  Saldo after PR 2. A problem found there is fixed in a follow-up PR that still rides 0.71.0. Nothing is tagged until
  both iPhone checks pass.
- Saldo's data on the maintainer's devices need not survive. On the A32 I uninstall the old Saldo and install a local
  build; "data persists" is checked across a kill and relaunch of the new install only.

## Part A. iOS kept tabs (barbershop → template → in PR 1) and the demo theme fix

### A.1 Behaviour (the ADR-0057 rules, in iOS terms)

- A screen is **tabbed** when it has tabs with exactly one `selected`; its slot is that tab's index (the same
  `tabSlotOf` rule as Android). Untabbed screens render exactly as today.
- `ScaffoldView` holds the slots as state: per slot, the last scaffold screen the core showed in that tab. The selected
  slot always renders this frame's screen. Hidden slots keep their last screen and their SwiftUI view identity, which
  is what keeps their `ScrollView` offset and `@State`.
- A hidden slot is invisible, not hit-testable and hidden from accessibility. Its `send` drops every `Action`
  (evaluated live, as Android's `rememberUpdatedState(i == slot)`).
- A tab switch shows the new slot at once, with no transition. A route change inside one slot (push/pop) keeps today's
  `navTransition` slide. Moving between a tabbed and an untabbed screen animates as today.
- A switch ends editing (resigns the first responder), so the keyboard closes and no hidden field keeps focus.
- **Not kept when hidden** (`keptWhenHidden`, the same list as Android): a screen holding a `Video`, `Map`, `WebView` or
  `PdfView`; `refreshing`; an indeterminate `Progress`; a `LazyList` that is `loading` or `refreshing`; a nested
  `Scaffold`; a SECURE `TextField`. Such a slot is dropped when hidden and rebuilt on return.
- A slot whose `route` changed while it was away is rebuilt without a transition, never shown stale.
- Slots past the current tab count are dropped. Leaving the tabbed screens drops them all (as on Android).
- The top bar, sheet, FAB, snackbar, bottom bar and tab bar keep rendering from the current screen, as today.

### A.2 Colours on reveal

iOS reads the palette and theme from statics (`ActivePalette.current`, `ActiveTheme.current`) that `Core.swift` sets
on every update, plus `\.paletteRoles` in the environment. A revealed slot must show the current colours, even when
its own tree is unchanged since it was hidden. The builder chooses how (for example a palette-derived identity or
environment value that invalidates the slot), and the plan verifies it with a theme change while a tab is hidden.

There is no iOS counterpart of the Android theme bug: SwiftUI has no static composition local that recomposes the app
on a new scheme object. The plan confirms this by reading `Core.swift` / `ScaffoldView`, and records it in the PR.

### A.3 Demo theme fix (Android)

`ui/theme/Theme.kt` in coffee, todo and fullstack-todo gets barbershop's fix: the colour scheme, typography and shapes
are each `remember`ed on the inputs they are built from. Nothing else in those demos changes. Saldo gets it through
Part B.

### A.4 Record and test

- ADR-0058 with a conformance test in `xtask/tests/adr_conformance.rs` over barbershop's and the template's
  `Render.swift`. It asserts, in the spirit of ADR-0057's test: the hidden slot's send is gated on "is selected", a
  hidden slot is hidden from hit testing and accessibility, the switch ends editing, `holdsLiveView` names all four
  live kinds, `keptWhenHidden` checks every clause of A.1, and the slot host asks `keptWhenHidden`. Proven by
  mutations recorded in the ADR's §4.
- READMEs: grep for tab / cross-fade / scroll-position wording (root `README.md`, `mobiler/README.md`,
  `mobiler-ui/README.md`) and update each match; list them in the PR body.

### A.5 Verification

- CI builds the iOS lanes; nothing iOS can be built locally.
- The maintainer checks barbershop on iPhone through the TestFlight (barbershop) lane: scroll a tab, switch, return;
  type in a field, switch (keyboard closes); VoiceOver doesn't reach the hidden tab; Profile's web view and a video
  still behave; dark mode flip while another tab is hidden.
- Android demos: each builds (CI), and coffee or todo is run on the A32 once to see it still renders.

## Part B. Saldo fully current (PR 2, after PR 1 merges)

### B.1 What Saldo becomes

A normal CLI app, as if created today:
- `mobiler.toml` (name, app id `rs.mobiler.saldo`, and `[splash]` / `[fonts]` / `[deep_links]` only if Saldo uses them
  today).
- Shells generated by the current CLI (`mobiler new` with Saldo's name and package), then `mobiler plugin add` for
  `biometric`, `filepicker`, `files` and `sqlite`. Its plugin copies today differ from the bundled ones only by the
  package line, so this is a clean swap.
- The committed `.mobiler/` state (version stamp + `base/` snapshot), so `mobiler upgrade` is a no-op right after.

### B.2 What carries over (Saldo's own)

- App icons, launcher resources, `strings.xml` app name, the iOS `Assets.xcassets` icon, and display names.
- The TestFlight (saldo) and Android APK (saldo) workflows and their build scripts' expectations (paths, bundle id,
  signing). The workflows are adjusted to the new layout, not replaced.
- **Release signing moves out of the shell** (my call). Saldo's `app/build.gradle.kts` signs `release` with the debug
  key so the CI APK sideloads; the template doesn't, and the file is a shell file, so the line can't stay. The APK
  workflow signs the release APK itself (`apksigner` with a debug keystore), and the shell file equals the template.
- Its Rust core (`shared/`), unchanged except what the template's `shared/` files force (for example the codegen
  binary or build files).
- Any edit found in Saldo's current shells that isn't in the template: the builder diffs each Saldo shell file against
  the template rendered with Saldo's names. Each difference is either already done by the template (dropped), Saldo's
  own (kept, preferably as config or a plugin rather than a shell edit), or a framework improvement that belongs in the
  template (reported to the maintainer, not silently kept). The PR body lists them.
- The Rust dependency form stays what an in-repo demo needs (path deps on `mobiler-core` / `mobiler-ui`, as
  barbershop), so CI builds Saldo against the workspace libraries.

### B.3 Record and test

- ADR-0059 (Saldo is CLI-managed and upgraded in each shell-changing CLI PR). Its conformance test (my call: the strict
  form) renders the **current** template with Saldo's names and compares Saldo's shell files with it, apart from
  plugin anchor lines. Comparing against `.mobiler/base/` would only catch hand edits, never a forgotten upgrade.
  The cost: every future PR that changes a template shell fails CI until it runs `mobiler upgrade --apply` on Saldo.
  That is the point of the record. Proven by mutation.
- The release skills (`release-cli`, and `ship-pr`'s checklist if it lists demos) gain the "upgrade Saldo" step.

### B.4 Verification

- `mobiler upgrade` on Saldo reports nothing to do; `mobiler doctor` is clean.
- CI's Saldo lanes build.
- On the A32 (I drive it, after checking the foreground app): launch, unlock with biometric, add and edit a bill,
  switch through all four tabs (kept scroll), export a file, pick a file, switch language and dark mode, kill and
  relaunch (data persists in SQLite).
- The maintainer runs the TestFlight (saldo) lane and checks the same on iPhone.

## Risks

- **iOS hidden slots still lay out.** SwiftUI keeps an opacity-0 view in layout, unlike Android's unmeasured slot. With
  up to four screens alive this could cost time on update. Mitigation: a hidden slot's inputs don't change while it is
  hidden, so SwiftUI should skip its body; the TestFlight check watches for sluggish typing or switching.
- **Stale colours on reveal** (A.2).
- **Saldo's existing data**: the app id stays `rs.mobiler.saldo`, so installed copies upgrade in place. The SQLite file
  doesn't move: the bundled sqlite plugin and Saldo's copy use the same `mobiler.db` location on both platforms
  (checked by the builder review).
- **First CLI-managed demo.** No demo has had a version stamp or `.mobiler/base/` before (barbershop's `.mobiler/`
  holds only `splash.json`), so upgrade's behaviour on an in-repo demo with path dependencies is new ground. Path
  dependencies are left alone by upgrade (ADR-0056); other findings are expected and are fixed in the CLI, in PR 2.
- **Saldo's shell edits** may include behaviour the maintainer relies on that isn't obvious from code. B.2's listed
  diff is how it surfaces.
- **0.71.0 grows.** Both PRs ride the same unreleased version, so the release waits for both.
