# ADR-0005: An update's `Render` is emitted before its requests, notifications and streams, and shells never block the UI on a plugin call

Status:        Accepted
Date decided:  2026-09-21
Deciding PRs:  #208
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs (MobilerShell update → effect order), Android Core.kt (per-request coroutine), shells' plugin dispatch
Conformance:   mobiler-core/src/lib.rs::shell_renders_before_requests_notifications_and_streams

## 1. Context (The Problem)

The appointments team saw typed text lag and a "Saving…" state appear only after the network
answered. The core emitted its `Render` *after* the update's plugin requests, and Android awaited
each request inline. So the new UI state waited a full network round trip.

## 2. Hypothesis

If the core emits `Render` first, and every shell runs each plugin request independently (never
awaiting it on the render path), then:

- the user sees the result of their action immediately, whatever the network is doing;
- requests still complete and resolve their continuations as before.

### 2.1. Refutation Conditions

- **Condition 1 — render comes first.** For an update that queues a request, a notification and a
  stream, the effects start with `Render`.
  - **Validation Metric:** `shell_renders_before_requests_notifications_and_streams` in
    `mobiler-core/src/lib.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep the order; tell apps to set "saving" in a separate update** `[recorded: docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md]`
  Rejected: every app would pay the complexity, and inline awaiting on Android would still stall.
- **Option B — render first, requests concurrent** `[recorded: same spec; PR #208]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option B, shipped as core 0.35.1 / CLI 0.52.1. On the emulator, fast typing kept every keystroke and
"Saving…" appeared before the reply. The baseline `main` had lost input.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the UI is never stalled by I/O.
- **Negative:** a request's continuation can arrive after later user input, so an app handling a
  result must not assume the model is unchanged since it was sent. This is why the snackbar undo
  carries its own payload (see the snackbar spec).
- **Negative:** text fields needed local editing state (`FieldSync` on Android) so a render's echo
  of an older value doesn't overwrite newer keystrokes.
