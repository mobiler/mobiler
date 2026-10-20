# Web keeps tab positions; every demo but barbershop is CLI-managed; deferred minors

**Asked by:** the maintainer, 2026-10-20: do items 2 (web kept tabs), 5 (bring coffee, todo, fullstack-todo up to date,
CLI-managed like Saldo) and 6 (the deferred minors), "split it into two PRs, go ahead". On the demos' guard: "Strict,
like Saldo".
**Release:** PR 1 → `mobiler-web` 0.49.0 (libraries). PR 2 → CLI 0.73.0.
**Constrained by:** ADR-0001 (generic shells), ADR-0015 (barbershop is the reference), ADR-0023 (the web shell is the
`mobiler-web` library), ADR-0032 (no shell navigation state beyond a render cache), ADR-0057 / ADR-0058 (kept tabs on
Android / iOS), ADR-0059 (platform-only options), ADR-0060 (Saldo is CLI-managed; superseded here).

## PR 1 — `mobiler-web` 0.49.0

### Web tab positions (ADR-0062)

The web shell rebuilds the whole DOM on every core update (a Leptos signal over the root `Widget`), so it has no live
screen to keep. ADR-0057/0058's user-visible behaviour, done the web's way (my call):
- **Scroll position per tab.** When the selected tab changes, the shell saves the scroll offset of the screen it
  leaves (page scroll, or the scaffold body / fill list when that is what scrolls) under that tab's slot and route, and
  restores the saved offset for the tab it shows, after the new DOM is in place. A slot whose route changed while away
  starts at the top (ADR-0057's "rebuilt, never stale" rule). Leaving the tabbed screens drops the saved offsets.
- **No transition on a tab switch.** The body's route animation runs for a push/pop inside a tab, never for a switch.
- **A switch drops focus** (`document.activeElement.blur()`), so no field on the old tab keeps it.
- Same slot rule as the native shells: a screen is tabbed when exactly one tab is selected.
- Nothing is kept that could run while hidden: the web never has hidden screens.

### Web minor: icon-only buttons

Icon-only buttons and the FAB render the glyph as text, so a screen reader speaks the emoji's name ("purse" for
`Wallet`). The glyph gets `aria-hidden="true"` and the button an `aria-label`: the widget's accessible label where it
has one, else the icon's English name (`"Wallet"`, `"Add"`), the same names the iOS shell's VoiceOver reads.

### Release

`mobiler-web` 0.49.0 (a behaviour change, so a minor bump an app takes with `mobiler upgrade`). `mobiler-ui` and
`mobiler-core` don't change. Barbershop, coffee and todo's web clients build against the workspace.

## PR 2 — CLI 0.73.0

### Every demo but barbershop is CLI-managed (ADR-0063, supersedes ADR-0060)

Coffee (`demos/coffee`), todo (`demos/todo`) and fullstack-todo's mobile app (`demos/fullstack-todo/mobile`) become what
Saldo is: shells from `mobiler new` + `mobiler plugin add`, `.mobiler/` committed, `mobiler upgrade` reports Up to date,
and a strict conformance test, generalised from ADR-0060's, over all four demos. Barbershop stays the reference shell
the template is ported from (ADR-0015).
- **Audit first, by commit and content** (the Saldo lesson): every commit that touched each demo's shells is read, and
  each demo-only behaviour is mapped to the template, a bundled plugin, the demo's Rust theme/palette, or a gap.
  Known: coffee's terracotta theme lives in its `Theme.kt` (hand edit) and moves into a `Theme.palette` in its Rust.
  A gap that needs a framework feature stops the work and goes to the maintainer.
- The demos keep their own icons, names and Rust cores; the release-cli step upgrades all four.

### Native minors

- **Android picker hang on activity recreation** (a dark-mode switch with the dialog open): the picker dismisses its
  dialog and answers `ok:false` when the activity is destroyed (a lifecycle observer), so the request never hangs.
- **The band between a bottom bar and the glass tab bar (iOS):** the space under the bottom bar takes the bar's fill,
  so the bar reads as one surface down to the capsule.
- **A swipe row inside a sheet or card:** the cover takes the colour of the surface it sits on (an environment value
  the sheet and cards set), not always the page background.

## Non-goals

- Barbershop as a CLI-managed app (it is the reference).
- fullstack-sqlx (no native shells).
- The other deferred cosmetic Saldo minors (text-button padding, press animation, sidebar tint, dark chip shade).

## Anti-goals

- A demo losing any behaviour in the regeneration (the audit's job).
- A web tab switch that shows the old tab's content at a stale scroll position of a different route.

## Left to the builder

- The exact web scroll container detection and the restore timing (a post-render `request_animation_frame`).
- The test shape for the web position logic (a pure function over slot/route keys, unit-tested).
- Each demo's palette values (from its current hand-edited theme).
