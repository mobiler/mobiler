# ADR-NNNN: <the decision, stated as a claim sentence>

Status:        Accepted
Date decided:  YYYY-MM-DD
Deciding PRs:  #NNN
Supersedes:    none
Code anchor:   <the modules or files this decision governs>
Conformance:   <comma-separated test paths (with test names), or "none — <why>">

## 1. Context (The Problem)

What was true that made a decision necessary: the constraint, the pressure, or the failure that
forced a choice.

## 2. Hypothesis

The decision stated as a falsifiable claim: if we do this, these properties hold.

### 2.1. Refutation Conditions

Each condition names what would prove the decision wrong, and the test that would catch it.

- **Condition 1 — <short name>.**
  <what must be true>
  - **Validation Metric:** <the assertion, and the test path>

## 3. Considered Options & Rationale for Refutation

Every option carries a provenance tag: `[recorded: <source>]` or `[reconstructed]`.

- **Option A — <name>** `[reconstructed]`
  <what it was, and why it was not chosen>

## 4. Decision & Rationale for Corroboration

What we chose, and what evidence supports it holding up. If an invariant test guards it, add a
**Mutation proof:** say what was broken on purpose and how the test failed.

## 5. Consequences (Positive and Negative Predictions)

What this makes easy, what it makes hard, and what it rules out. Negative consequences are required:
a record with only positives has not been thought through.
