# ADR-0032: Which screen, tab, sheet or detail pane is shown is app model state carried in the widget tree; a shell holds no navigation state, only layout and animation bookkeeping, and every back affordance a shell draws or wires fires the app's event

Status:        Accepted
Date decided:  2026-05-26
Deciding PRs:  none (pre-PR history — commit fe718c1 "Add a navigation model: core-owned Nav stack, animated shell transitions + system back", merged by 55fad34); held by #136 (`Widget::Split`, 2026-06-08)
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs (Nav, nav_scaffold, scaffold_back, split), mobiler-ui/src/lib.rs (Widget::Scaffold route/depth/back/tabs/sheet, Tab, Widget::Split), template Android MainActivity.kt (BackHandler, Split arm), template iOS Render.swift (scaffold prevDepth + edge-swipe back, SplitView), mobiler-web/src/lib.rs (thread_local NAV, Split arm)
Conformance:   mobiler-core/src/lib.rs::nav_scaffold_shows_back_only_when_poppable, mobiler-core/src/lib.rs::nav_push_pop_depth

## 1. Context (The Problem)

The todo port needed a detail screen pushed over its tabs, a hardware back button on Android, and
animated transitions. Native toolkits each bring their own navigation state (a Compose back stack,
a SwiftUI `NavigationStack` path, browser history). If a shell kept the stack, the core's `view`
would no longer decide what is on screen. The shells are generic and shared by every app
(ADR-0001), so any per-app navigation logic in them was out.

## 2. Hypothesis

If the app keeps its navigation in its `Model` (a `Nav<R>` stack, the selected tab, whether a sheet
or detail pane is open), `view` encodes it in the tree (`Scaffold.route`, `depth`, `back`,
`Tab.selected`, `Scaffold.sheet`, `Split.show_detail`), and the shells turn every back or dismiss
gesture into the app's own event, then:

- the core is the single source of truth for what is shown, on all three platforms;
- navigation is testable in plain Rust, with no shell;
- the shells stay stateless renderers that no app edits.

### 2.1. Refutation Conditions

- **Condition 1 — the tree carries the stack.** `nav_scaffold` fills `route` and `depth` from the
  `Nav` and sets `back` only when the stack can pop.
  - **Validation Metric:** `nav_scaffold_shows_back_only_when_poppable` in `mobiler-core/src/lib.rs`.
- **Condition 2 — the stack is plain app data.** `push`/`pop`/`reset` behave as a stack, and `pop`
  at the root is a no-op.
  - **Validation Metric:** `nav_push_pop_depth` in `mobiler-core/src/lib.rs`.
- **Condition 3 — shells hold no navigation state.** Not tested: it lives in three shells' render
  code. Review (see §4 for what a shell may hold).

## 3. Considered Options & Rationale for Refutation

- **Option A — each shell owns a native navigation stack** `[reconstructed]`
  `NavigationStack` / Compose navigation / browser history would own the stack. The core would
  have to mirror it through events, and the three platforms could disagree about what is shown.
- **Option B — the core owns the stack and the shell renders the top screen** `[recorded: commit fe718c1 ("The core owns the nav stack (single source of truth)"; "navigation is a property of the ABI, free for every app"); mobiler-core/src/lib.rs `Nav` doc ("The **core owns the stack**"); mobiler/agentic/CLAUDE.md ("The core owns the stack; the shell animates it.")]`
  Chosen.
- **Option C — for `Split`, send the screen's size class to the core so it picks the layout** `[recorded: PR #136 ("the core needs **no** size-class signal (deferred)")]`
  Not done. The shell picks one pane or two from the width; see §4.

## 4. Decision & Rationale for Corroboration

Option B. The app holds a `Nav<R>` in its model and builds the root with `nav_scaffold(…, &nav,
on_back)`, which sets `route` (the current route's serialization), `depth`, and `back` (only when
`can_go_back()`). Tabs are `Tab { selected, on_select }`, the bottom sheet is `Scaffold.sheet` with
its `on_dismiss`, and a master-detail's phone pane is `Split.show_detail` with `on_back`. The shell
turns each gesture into the app's token: the top-bar arrow, Android's `BackHandler` (only when
`back` is set; at the root the system back exits the app), the iOS leading-edge swipe, a tab tap,
a scrim tap, the `Split` back control (a chevron on iOS, a "‹ Back" text button on Android and
web).

What a shell **may** hold:

- transition bookkeeping keyed on `route`/`depth`: iOS `@State prevDepth`, Android's
  `AnimatedContent` comparing old and new `depth`, and web's `thread_local!
  NAV` (previous route key, previous depth, a toggle). The web comment reads "stateless whole-tree
  rebuild, so nav state lives here"; that state only picks the animation direction;
- layout from the screen size: the nav rail instead of bottom tabs on a wide screen, and `Split`
  side by side on a wide screen, where `show_detail` and `on_back` are ignored (`Widget::Split` doc);
- UI state inside one widget (scroll offset, a text field's local edit, ADR-0005) and the
  shell-owned confirm and snackbar (ADR-0017).

What a shell **may not** hold: a stack, a current route, a selected tab, or an open/closed flag for
a sheet or detail pane. The same rule for live native views is ADR-0026.

`Widget::Split` (#136) kept the rule: the app sets `show_detail` on selection and clears it on
`on_back`.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app's navigation is ordinary Rust, unit-testable, and identical on every shell.
- **Positive:** deep links and restore can put the app on any screen by setting model state.
- **Negative:** every app must wire back itself: a `Msg::Back` that calls `pop`. Forget it, and
  the arrow and system back do nothing useful.
- **Negative:** only `Scaffold.back` (Android `BackHandler`, iOS edge swipe) and, on Android, an
  open sheet (Material dismisses it on back, firing `on_dismiss`) are wired to the system back. A
  `Split` detail on a phone is closed by its back control; the system back there exits or pops
  the scaffold unless the app also sets `Scaffold.back`.
- **Negative:** the web shell does not push browser history. The browser's back button does not
  fire the app's back event, and the URL doesn't follow the screen.
- **Negative:** `Nav<R>` derives only `Clone` and `Debug`, not `Serialize`. An app that wants the
  screen to survive a restart (ADR-0031) must save the route
  itself.
- **Negative:** on a wide screen the shell, not the app, decides that both `Split` panes are
  visible, and the core can't tell which layout is showing.
