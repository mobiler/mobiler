# ADR-0006: Continuous native events reach the core through one keyed streaming primitive (`cx.subscribe` / `cx.unsubscribe`), shipped with its full lifecycle; streaming capabilities ride it instead of adding ABI

Status:        Accepted
Date decided:  2026-06-05
Deciding PRs:  #113
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs (Cx::subscribe / unsubscribe, Effect::PluginStream, subscribe_appearance), shells' `subscribe` in plugin registries
Conformance:   mobiler-core/src/lib.rs::cx_subscribe_enqueues_a_keyed_stream_and_maps_each_event, mobiler-core/src/lib.rs::cx_unsubscribe_enqueues_the_teardown_notify_keyed_by_subscription

## 1. Context (The Problem)

Request/response (ADR-0002) answers once. Sensors, WebSocket messages, BLE notifications, transfer
progress, geofences and the OS light/dark setting all produce *many* events over time. Each needs
start and stop, and a stop that really tears down the native source.

## 2. Hypothesis

If there's one streaming primitive (a keyed subscription whose continuation fires once per event,
plus an unsubscribe that tears the native source down), then every streaming capability can be
built on it without a new effect or ABI change, and none leaks a running native source.

### 2.1. Refutation Conditions

- **Condition 1 — each event maps through the continuation.**
  - **Validation Metric:** `cx_subscribe_enqueues_a_keyed_stream_and_maps_each_event`.
- **Condition 2 — unsubscribe reaches the shell, keyed by the subscription.**
  - **Validation Metric:** `cx_unsubscribe_enqueues_the_teardown_notify_keyed_by_subscription`.

## 3. Considered Options & Rationale for Refutation

- **Option A — one bespoke effect per streaming capability** `[reconstructed]`
  An ABI change and a coordinated release per capability, against ADR-0002.
- **Option B — polling via repeated requests** `[reconstructed]`
  Wasteful, laggy, and impossible for push-style sources like BLE notify or WebSocket.
- **Option C — a keyed streaming primitive with full lifecycle** `[recorded: PR #113; retrofits PR #115 (websocket), #195 (transfers, "ride cx.subscribe — no ABI break")]`
  Chosen, and shipped with *both* subscribe and unsubscribe in v1. The maintainer's standing rule
  is that a primitive ships with its full lifecycle.

## 4. Decision & Rationale for Corroboration

Option C. The websocket, BLE notify, streaming transfers (upload/download progress, cancel =
unsubscribe) and `cx.subscribe_appearance` have all since been built on it without touching the ABI.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** new streaming capabilities are shell-plus-core work only, and cancellation is free.
- **Negative:** each shell must implement real teardown for every streaming plugin. A shell that
  ignores unsubscribe leaks a sensor or socket, and nothing at the ABI level can catch that.
- **Negative:** stream payloads share the stringly-typed boundary of ADR-0002.
