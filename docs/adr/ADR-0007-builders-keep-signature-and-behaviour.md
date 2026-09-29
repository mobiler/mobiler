# ADR-0007: An existing core builder keeps its signature and its behaviour: a plugin call's input payload stays byte-identical, and a widget builder leaves new fields at their no-op defaults; new behaviour arrives as a new builder or `with_*` modifier

Status:        Accepted
Date decided:  2026-06-04
Deciding PRs:  #102, #210
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs, mobiler-core/src/dialog.rs (builders: text_field / field, confirm / confirm_with, with_fab / with_extended_fab, open_url / open_url_then, badge + with_icon, …)
Conformance:   mobiler-core/src/lib.rs::cx_confirm_stays_byte_identical_on_the_wire, mobiler-core/src/lib.rs::open_url_then_is_a_request_and_open_url_stays_a_notification, mobiler-core/src/lib.rs::with_extended_fab_sets_the_label_and_with_fab_does_not

## 1. Context (The Problem)

Apps call builders like `text_field`, `cx.confirm`, `with_fab` and `cx.open_url` everywhere. When a
capability grows (keyboard kinds on a text field, labels on a confirm, a label on a FAB, a result for
`open_url`), changing the existing builder's arguments would break every call site. For plugin calls
there's a second risk: changing the payload the old builder sends would change what an older shell
has to parse.

Two different kinds of builder are involved, and they can't make the same promise:

- A **plugin-call builder** (`cx.confirm`, `cx.open_url`) sends an input payload that the shell
  parses. That payload can stay exactly the same.
- A **widget builder** (`text_field`, `with_fab`) builds an ABI value. When the ABI type gains a
  field (ADR-0008), the value's bytes necessarily change. What can stay the same is what it
  *renders*: the new field is left at its no-op default. PR #102 is the example: `Widget::TextField`
  gained `kind` and `error`, and `text_field(id, ph, value)` kept its signature while leaving them
  at `Plain` / `None`, so existing screens rendered exactly as before.

## 2. Hypothesis

If existing builders never change signature; plugin-call builders keep their exact input payload;
widget builders leave new fields at their no-op defaults; and new capability arrives as a sibling
builder (`field`, `confirm_with`, `open_url_then`, `with_extended_fab`) or a set-field-or-no-op
modifier (`with_icon`, `with_columns`, `with_step_caption`, …), then:

- upgrading never breaks an app's existing calls;
- existing screens render as before;
- for plugin calls, a shell that predates the new builder still parses the old call.

### 2.1. Refutation Conditions

- **Condition 1 — a plugin call's payload is unchanged.**
  - **Validation Metric:** `cx_confirm_stays_byte_identical_on_the_wire`.
- **Condition 2 — the old call keeps its kind.** For example, `open_url` stays a fire-and-forget
  notification while `open_url_then` is a request.
  - **Validation Metric:** `open_url_then_is_a_request_and_open_url_stays_a_notification`.
- **Condition 3 — the old widget builder leaves the new field at its default.**
  - **Validation Metric:** `with_extended_fab_sets_the_label_and_with_fab_does_not`.

## 3. Considered Options & Rationale for Refutation

- **Option A — extend the existing builder's arguments** `[reconstructed]`
  It would break every call site for a feature most calls don't use.
- **Option B — keep the old builder; add sibling builders and `with_*` modifiers** `[recorded: PR #102 ("text_field(id, ph, value) kept byte-compatible … existing demos render exactly as before", plus the new field(..) / *_field builders); PR #210 ("Plain cx.confirm stays byte-identical on the wire"); docs/superpowers/specs/2026-09-29-open-url-result-design.md]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option B. It has held through the rich form fields, `confirm_with`, the picker labels, the extended
FAB, `open_url_then` and the design release's `with_*` modifiers, with no app-facing builder
changing signature.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** app code keeps compiling across upgrades, existing screens render as before, and old
  plugin calls stay parseable by shells that predate the new builder.
- **Negative:** the builder surface grows: pairs like `x` / `x_with` / `x_then`, plus many `with_*`
  modifiers. Discoverability suffers without good docs.
- **Negative:** this protects *builders*, not wire bytes of widgets or struct literals. An app that
  writes an ABI struct in full still breaks when a field is appended, and the core and shells must
  be upgraded together (ADR-0008).
