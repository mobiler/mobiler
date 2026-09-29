# ADR-0017: At most one shell-owned confirm and one snackbar are open at a time; a new one answers the open one `ok: false` and replaces it, so every call's continuation resolves exactly once

Status:        Accepted
Date decided:  2026-09-22
Deciding PRs:  #210 (first instance: Android `ConfirmHost`, demo shells), #213 (the rule on every shell); extended to the snackbar by #232; templates ported in #211, #214, #216 and #242
Supersedes:    none
Code anchor:   template Android Core.kt (ConfirmHost, DialogPlugin, SnackbarBus, SnackbarPlugin); template iOS Core.swift (DialogPlugin.openConfirm, SnackbarHost.show, SnackbarPlugin); mobiler-web/src/lib.rs (OPEN_CONFIRM + confirm_modal, OPEN_SNACKBAR + show_snackbar)
Conformance:   none — the behaviour lives in Kotlin, Swift and web DOM code with no unit-test harness; it is checked by the runtime checks recorded in PRs #213 and #232 and by review

## 1. Context (The Problem)

`cx.confirm` and `cx.snackbar` are requests: the app's `then` runs when the user answers. An app can
fire a second one while the first is still on screen, for example two quick swipe-to-cancels, each
showing an "Undo" snackbar. Each shell then needs a rule, or the shells diverge: a second alert
can fail to present on iOS, two web modals can stack so one Escape closes the wrong one, and M3 on
Android queues snackbars behind each other. Worse, a continuation that is never answered leaves the
app waiting forever.

## 2. Hypothesis

If each shell keeps at most one confirm and at most one snackbar, and a new one first answers the
open one `ok: false` and removes it, then:

- the user sees only the newest question or message;
- every `then` runs exactly once, so no app state waits on an answer that never comes;
- the behaviour is the same on Android, iOS and web.

### 2.1. Refutation Conditions

- **Condition 1 — a superseded confirm resolves `ok: false`.** On all three shells its output is
  `"cancel"`, the same as a user cancel.
  - **Validation Metric:** review. Web CDP check recorded in PR #213: "Two confirms leave one modal."
    The spec's acceptance criterion: "Two confirms fired in a row leave exactly one modal on screen;
    the first resolves `false`."
- **Condition 2 — a replaced snackbar resolves `ok: false, "replaced"`.**
  - **Validation Metric:** review. Android emulator check recorded in PR #232: "a second cancel
    replaces the snackbar in under 1 s, and its Undo restores only the second booking."
- **Condition 3 — no call is left unanswered.** Including the iOS edge cases: a confirm queued
  behind a dismissal, or a present that fails.
  - **Validation Metric:** review of `DialogPlugin.presentIfStillCurrent` (iOS), which answers an
    alert that can no longer be presented instead of dropping it (PR #215).

## 3. Considered Options & Rationale for Refutation

- **Option A — stack or queue them** `[recorded: docs/superpowers/specs/2026-09-28-snackbar-design.md, Out of scope: "Queuing (we replace instead)."]`
  M3's `SnackbarHostState` queues by default. A queue shows stale feedback after the moment has
  passed, and stacked modals make one Escape or back press ambiguous. Rejected.
- **Option B — refuse the new one while one is open (answer the *new* call `ok: false`)** `[reconstructed]`
  The user would never see the newest question, which is usually the one that matters. Nobody
  wrote this option down.
- **Option C — the new one supersedes the open one, which answers `ok: false`** `[recorded: docs/superpowers/specs/2026-09-22-shell-labels-catalog-design.md, C ("**Rule:** a new confirm answers the open one `ok: false` / `"cancel"` and replaces it. Android already does this through `ConfirmHost`."); snackbar spec decision 3 ("This matches the one-confirm-at-a-time rule.")]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. What each shell does now:

- **Confirm, Android** (`Core.kt`, since #210): `ConfirmHost.pending?.answer(false)` runs before the
  new request is published. The old call resolves `PluginResponse(false, "cancel")`.
- **Confirm, iOS** (`Core.swift`, #213, hardened in #215): `openConfirm` names the alert on screen.
  A new confirm answers it `PluginResponse(ok: false, output: "cancel")`, unless it is already being
  dismissed (the user just tapped it, and its own handler answers). The new alert is presented only
  after the old dismissal completes. If it can no longer be presented, it is answered instead of
  dropped.
- **Confirm, web** (`confirm_modal`, #213): once the new modal is built, the old one's sender gets
  `false` (the call answers `ok: false, "cancel"`) and is marked superseded, so its teardown does
  not steal focus. The new modal inherits the first dialog's focus-return target.
- **Snackbar, all three** (#232): the visible or queued snackbar answers `ok: false, "replaced"` and
  is removed. Android cancels the in-flight `showSnackbar` job (`SnackbarBus.current`), iOS calls
  `finish("replaced")` in `SnackbarHost.show`, and web sends `"replaced"` on `OPEN_SNACKBAR`.

Pickers are outside this rule (spec 2026-09-22, C: "Pickers are out of scope.").

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app can rely on every confirm and snackbar `then` running exactly once, whatever
  the user or the app does next.
- **Positive:** the barbershop undo flow works with two quick cancels. The first snackbar's
  `ok: false` answer lets the app keep only the second booking's undo.
- **Negative:** a superseded confirm is indistinguishable from a user cancel (`ok: false,
  "cancel"`). An app that treats cancel as a deliberate "no" (logging it, say) will count a
  supersede as one. Snackbars say `"replaced"`; confirms don't.
- **Negative:** this is only as strong as three hand-written implementations. There is no
  automated test, and iOS needed a second release (#215) to present the replacement reliably;
  a device check of that timing was still listed as a follow-up in PR #215.
- **Negative:** date and time pickers do not follow the rule, so two picker calls behave however
  the platform does.
