# ADR-0010: One app's product or compliance rule never becomes a framework limitation; the capability ships as an opt-in plugin that apps which forbid it simply don't install

Status:        Accepted
Date decided:  2026-06-07
Deciding PRs:  #120
Supersedes:    none
Code anchor:   mobiler/plugins/** (opt-in bundled plugins, e.g. iap, push, analytics), `mobiler plugin add`
Conformance:   none — a scoping rule for what the framework offers; enforced in design review

## 1. Context (The Problem)

mobiler's first driver apps have their own product rules. The appointments admin app forbids in-app
purchase UI (an App Store compliance choice: B2B billing on the web). A plan for push was once scoped
"iOS + Android, no IAP" because of that app, and the maintainer corrected it: the framework should
support IAP, because other apps will want it.

## 2. Hypothesis

If an app-level "must not" is expressed as an **opt-in** plugin (nothing installed unless the app
runs `mobiler plugin add x`) rather than as a capability the framework lacks, then:

- apps with the rule stay compliant by never installing it;
- every other app still gets the capability;
- the framework's scope isn't set by its first customers' product decisions.

### 2.1. Refutation Conditions

- **Condition 1 — a forbidden capability is absent by default.** A fresh scaffold includes no
  opt-in plugin until it is added. Review checks this when a plugin is added.

## 3. Considered Options & Rationale for Refutation

- **Option A — leave the capability out because the driver app forbids it** `[recorded: maintainer correction, "mobiler as framework should support IAP since some other apps can use it if they wish"]`
  Rejected: it makes one app's policy everyone's limitation.
- **Option B — ship it built in, on by default** `[reconstructed]`
  Rejected: a compliance-sensitive app would then have to strip it out.
- **Option C — an opt-in bundled plugin** `[recorded: PR #120 (iap)]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. `iap` (StoreKit 2 / Play Billing) shipped as an opt-in plugin in CLI 0.33 (PR #120). The
same pattern covers `push`, `analytics` and the background capabilities. The appointments app
doesn't install `iap` and stays compliant.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the framework's reach grows with the market, not with one app's rules.
- **Negative:** more plugins to maintain and keep in step with the shells (the "plugin drift" risk
  noted in ADR-0002).
- **Negative:** the decision needs judgement each time: which constraints are "app policy" and which
  are genuine framework safety limits. This record doesn't settle that for every case.
