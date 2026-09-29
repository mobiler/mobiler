# ADR-0007: An existing core builder keeps its signature and its exact wire output; new behaviour arrives as a new builder or `with_*` modifier

Status:        Accepted
Date decided:  2026-06-04
Deciding PRs:  #102, #210
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs, mobiler-core/src/dialog.rs (builders: confirm / confirm_with, with_fab / with_extended_fab, open_url / open_url_then, badge + with_icon, …)
Conformance:   mobiler-core/src/lib.rs::cx_confirm_stays_byte_identical_on_the_wire, mobiler-core/src/lib.rs::open_url_then_is_a_request_and_open_url_stays_a_notification, mobiler-core/src/lib.rs::with_extended_fab_sets_the_label_and_with_fab_does_not

## 1. Context (The Problem)

Apps call builders like `text_field`, `cx.confirm`, `with_fab` and `cx.open_url` everywhere. When a
capability grows (labels on a confirm, a label on a FAB, a result for `open_url`), changing the
builder's arity or its emitted payload breaks every call site, or silently changes what old shells
receive. The rich form fields release first hit this, and the fix was to keep the old builder
byte-compatible rather than bump its arity.

## 2. Hypothesis

If existing builders never change signature or wire output, and new capability arrives as a sibling
builder (`confirm_with`, `open_url_then`, `with_extended_fab`) or a set-field-or-no-op modifier
(`with_icon`, `with_columns`, `with_step_caption`, …), then upgrading never breaks an app's calls,
and an older shell still understands what a new core sends for the old call.

### 2.1. Refutation Conditions

- **Condition 1 — the old call's bytes are unchanged.**
  - **Validation Metric:** `cx_confirm_stays_byte_identical_on_the_wire`.
- **Condition 2 — the old call keeps its kind.** For example, `open_url` stays a fire-and-forget
  notification while `open_url_then` is a request.
  - **Validation Metric:** `open_url_then_is_a_request_and_open_url_stays_a_notification`.
- **Condition 3 — the old builder leaves the new field unset.**
  - **Validation Metric:** `with_extended_fab_sets_the_label_and_with_fab_does_not`.

## 3. Considered Options & Rationale for Refutation

- **Option A — extend the existing builder's arguments** `[recorded: rich form fields, PR #102 — "keep the old builder byte-compatible instead of bumping arity"]`
  Rejected: breaks every call site for a feature most calls don't use.
- **Option B — sibling builders and `with_*` modifiers** `[recorded: PR #210 (confirm_with / pick_*_with); open_url_then spec docs/superpowers/specs/2026-09-29-open-url-result-design.md]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option B. It has held through `confirm_with`, the picker labels, the extended FAB, `open_url_then`
and the design release's `with_*` modifiers, with no app-facing builder changing signature.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** app code keeps compiling across upgrades, and old payloads stay valid for old shells.
- **Negative:** the builder surface grows: pairs like `x` / `x_with` / `x_then`, plus many `with_*`
  modifiers. Discoverability suffers without good docs.
- **Negative:** this protects *builders*, not struct literals. An app that writes an ABI struct in
  full still breaks when a field is appended (ADR-0008).
