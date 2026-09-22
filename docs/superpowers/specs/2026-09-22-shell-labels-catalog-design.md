# App-wide shell labels, one confirm at a time, modal focus trap, demo lints — design

**Date:** 2026-09-22.
**Base:** `main` @ `8b55fa3`, with ui 0.25.0 / core 0.36.0 / web 0.36.0 and CLI 0.53.0 published.
**Origin:** the follow-ups from Release 2 (see `docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md` and NOTES.md, "Shell labels"). Agreed with the user on 2026-09-22.
**Constraint (unchanged):** apps never patch generated shell code. Everything arrives through a release plus `mobiler upgrade`.

**Scope:** items A–D, shipped as Release 3 (ui 0.26 / core 0.37 / web 0.37 + CLI 0.54). Item E, `LazyList { fill }`, gets its own design after this.

---

## A. App-wide shell labels (`ShellLabels`)

### Problem

Some text is drawn by the shells themselves, and the app can't change it:
- Web LazyList controls: `"↻ Refresh"` and `"Load more"` (`mobiler-web/src/lib.rs` ~1658, ~1667).
- The `Split` (master–detail) back button shows visible English text on every shell: Android `"‹ Back"` (`MainActivity.kt` ~1014), iOS `Label("Back", …)` (`Render.swift` ~1028), web `"‹ Back"` (`lib.rs` ~1629).
- The Scaffold back button is icon-only everywhere. Its accessible name is English on Android (`contentDescription = "Back"`, ~1223), and missing on iOS (the `chevron.left` Image, ~1121) and web (`"‹"`, ~1882).
- The English fallbacks when a confirm or picker call gives no labels: `OK` / `Cancel` / `Done`.

### Design

**API (additive).**
- `ShellLabels` is a wire type, so it lives in `mobiler-ui` next to `Theme`. It derives `Facet, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq` and has public `Option<String>` fields. Its builder methods (`ShellLabels::new().back(..)…`) are an inherent `impl` in mobiler-ui too; `Rgb::new` is the precedent for an impl there.
- `mobiler-core` re-exports `ShellLabels` and adds `with_labels`:

```rust
/// Text the shells draw themselves, in the app's language. Set once on the scaffold with
/// [`with_labels`]; every field is optional and falls back to the built-in English.
pub struct ShellLabels {
    pub back: Option<String>,       // Split back-button text (all shells); Scaffold back-button accessible name
    pub load_more: Option<String>,  // web LazyList "Load more"
    pub refresh: Option<String>,    // web LazyList refresh control (shown as "↻ {refresh}")
    pub ok: Option<String>,         // confirm dialog confirm-button default
    pub cancel: Option<String>,     // confirm dialog + picker cancel-button default
    pub done: Option<String>,       // picker accept-button default
}
impl ShellLabels { new(); back(l); load_more(l); refresh(l); ok(l); cancel(l); done(l) }  // builder
pub fn with_labels(widget: Widget, labels: ShellLabels) -> Widget  // no-op on non-Scaffold
```

**ABI.**
- `Widget::Scaffold` gains `labels: Option<ShellLabels>` as its last field. That makes this mobiler-ui 0.26.0.
- Every scaffold builder sets `labels: None`. `with_theme`, `with_refresh` and the other scaffold combinators carry `labels` through.

**Precedence, for each piece of text:**
1. A label passed to that specific call (`confirm_with`, `Picker`).
2. The scaffold's `ShellLabels`.
3. Today's English default.

So `with_labels(…, ShellLabels::new().ok("U redu").cancel("Otkaži"))` also localizes a plain `cx.confirm`.

**How the shells get at the labels.** Each shell stores the current scaffold's labels in the same place it stores the theme, so the dialog code, which never sees the view, can read them:
- Android: an `activeLabels` holder, like `activeTheme`, set in `App()`.
- iOS: `ActiveLabels.current`, like `ActiveTheme.current`.
- Web: a `thread_local!` stash, set when the Scaffold renders.

When a render has no labels, the stash is cleared.

**Where each label goes:**

| Label | Android | iOS | Web |
|---|---|---|---|
| `back` | Split: `‹ {back}` text; Scaffold: back icon `contentDescription` | Split: `Label({back})`; Scaffold: `.accessibilityLabel({back})` | Split: `‹ {back}` text; Scaffold: `aria-label` on `‹` |
| `load_more` | — (no control) | — | LazyList "Load more" button |
| `refresh` | — (pull gesture) | — | LazyList refresh button, as `↻ {refresh}`; the scaffold `↻` gets `aria-label` |
| `ok` | confirm default | confirm default | confirm default |
| `cancel` | confirm + picker default | confirm + picker default | confirm default |
| `done` | picker accept default | picker accept default | — (browser picker) |

For Android pickers, today's rule stays: with no label from any source, the platform's own buttons remain.

### Out of scope

- Android's `"PDF: could not load PDF"`. The rest of that text is the platform's error message.
- Web iframe accessibility titles (`"PDF"`, `"Web"`).
- Both are noted as follow-ups.

## B. Web confirm modal focus trap

The modal's existing `keydown` handler also handles `Tab` and `Shift+Tab`:
- Focus cycles between the modal's focusable buttons: cancel, then confirm.
- If focus is outside the card, the next Tab moves it to the first button.

Escape, Enter, backdrop and focus restore are unchanged.

## C. One confirm at a time (all shells)

**Rule:** a new confirm answers the open one `ok: false` / `"cancel"` and replaces it. Android already does this through `ConfirmHost`.
- **Web:** a `thread_local!` holds the open modal's answer sender. A new `confirm_modal` sends `false` on it first, so the old future runs its normal teardown (listeners detached, scrim removed) before or while the new one mounts. As a result, one Escape can only close the current modal.
- **iOS:** `DialogPlugin` keeps a static reference to the alert it presented plus its resume closure. A new confirm dismisses the old alert (without animation), resumes it with `ok: false`, then presents the new one. The existing resume-once guard stays.

Pickers are out of scope.

## D. Pre-existing clippy lints in demos

Fix the default-lint warnings reported by `cargo clippy --all-targets -- -D warnings` in demos/coffee, demos/todo, demos/fullstack-sqlx and demos/barbershop, for example `needless_borrows_for_generic_args` and `let_and_return`.
- The fixes are behaviour-neutral and go in a separate commit.
- CI's demo clippy step stays non-gating, as `ci.yml` intends. Making it gating is a separate decision for the user.

---

## Showcase and acceptance

**Barbershop showcase.** It sets `with_labels(…)` with its own English wording, different from the defaults so the effect is visible, e.g. `back("Back to shop")`, `load_more("Show more")`, `refresh("Reload")`, `ok("Sure")`, `done("Pick")`. It also adds a plain `cx.confirm` somewhere, to show the scaffold default applying.
- **Coffee** stays unlabelled, proving the defaults are unchanged.

**Acceptance:**
1. **Web, barbershop.**
   - The LazyList shows "Show more", and the refresh control reads "↻ Reload".
   - The Services tab's `Split` back button reads "‹ Back to shop". Barbershop's root scaffold has no back button, so the scaffold back-button accessible name is checked by reviewing the code.
   - A plain confirm shows "Sure" / the app's cancel label.
   - Two confirms fired in a row leave exactly one modal on screen; the first resolves `false`.
   - Tab and Shift+Tab stay inside the modal.
   - There are 0 JS exceptions.
2. **Web, coffee.** Coffee sets no labels, so its plain confirm still shows OK / Cancel. The unlabelled defaults for "Load more", "↻ Refresh" and the back button are checked by reviewing the code. Coffee has no LazyList or back button, and barbershop is the only demo with them.
3. **Android, barbershop.**
   - A plain confirm uses the scaffold's `ok` label.
   - The Services `Split` back button reads "‹ Back to shop".
   - Picker accept uses `done` unless the call overrides it. The booking flow's per-call labels still win.
   - These checks avoid leaving the Profile tab, because of the pre-existing swiftshader RenderThread crash.
4. **iOS.** Compiled by CI; the logic is reviewed.
5. **Core unit tests.**
   - `ShellLabels` round-trips.
   - `with_labels` sets it and is a no-op elsewhere.
   - Every scaffold combinator preserves `labels`.
   - A plain `cx.confirm` is still byte-identical on the wire, because the labels are applied by the shell, not the core.

## Release shape

Same as Releases 1–2:
1. **PR-A**: libs, the 5 demos' shells, barbershop showcase, lints, versions, docs. The template is untouched. Rebase-merge.
2. `release-libs`: ui 0.26.0 → core 0.37.0 → web 0.37.0.
3. **PR-C**: port the template shells, pin core `"0.37"`, CLI 0.54.0. Then `release-cli` with tag `v0.54.0`, then post-release.

The user is asked before each merge, publish and tag.
