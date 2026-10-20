# ADR-0064: `mobiler.toml` `[ios] local_network = true` lets an iOS app load plain HTTP from local addresses only (`NSAllowsLocalNetworking`), switched off by default; the framework never offers arbitrary HTTP loads

Status:        Accepted
Date decided:  2026-10-20
Deciding PRs:  #308
Supersedes:    none
Code anchor:   mobiler/src/local_network.rs, mobiler/templates/iOS/project.yml (the `mobiler:local-network` markers), mobiler/src/deep_links.rs (`toggle`, shared)
Conformance:   mobiler/src/local_network.rs::the_template_ships_local_networking_off, mobiler/src/local_network.rs::on_switches_on_only_local_networking_and_off_restores_the_template, mobiler/src/local_network.rs::an_unknown_ios_key_or_missing_markers_refuse_and_write_nothing

## 1. Context (The Problem)

fullstack-todo's iOS app talks to its own server at `http://localhost:3000` during development. iOS's App Transport
Security can refuse plain HTTP, and the app's hand-edited `project.yml` carried an exception for it. When the demo
becomes CLI-managed (ADR-0063), its shells must equal the template, so the exception has to be something the
framework offers. The maintainer chose an option that allows local networking only (2026-10-20: "Add a local-network
option").

Android needs nothing: its debug-only network security config already allows the dev hosts (ADR-0038).

## 2. Hypothesis

If the template ships ATS's `NSAllowsLocalNetworking` switched off between markers, and `mobiler.toml`
`[ios] local_network = true` switches it on at build (as `[deep_links]` does, ADR-0051), then:
- a local-first or self-hosted app can reach `localhost` and `.local` names over HTTP (a LAN address on a device also
  needs iOS's Local Network permission, `NSLocalNetworkUsageDescription`, which this option doesn't set);
- every other HTTP load stays blocked;
- apps that don't ask are unchanged.

The key sits in an `[ios]` table because only iOS needs it (ADR-0059's naming rule for platform-only options).

### 2.1. Refutation Conditions

- **Condition 1: off by default.** The template's block has nothing live between the markers.
  - **Validation Metric:** `the_template_ships_local_networking_off`.
- **Condition 2: on means local networking only, and off restores the template exactly.** Switched on, the block's
  live lines are exactly `NSAppTransportSecurity:` and `NSAllowsLocalNetworking: true`, never
  `NSAllowsArbitraryLoads`.
  - **Validation Metric:** `on_switches_on_only_local_networking_and_off_restores_the_template`.
- **Condition 3: a bad key or a pre-0.73 app without markers writes nothing and says so.**
  - **Validation Metric:** `an_unknown_ios_key_or_missing_markers_refuse_and_write_nothing`.

## 3. Considered Options & Rationale for Refutation

- **Option A: `NSAllowsArbitraryLoads`** `[recorded: fullstack-todo's old hand-edited project.yml]`
  Rejected. It allows plain HTTP to any host, which App Review questions and which no app here needs.
- **Option B: leave fullstack-todo out of the CLI-managed demos** `[recorded: the maintainer's choice, 2026-10-20]`
  Rejected by the maintainer.
- **Option C: always allow local networking** `[reconstructed]`
  Rejected. An app that never talks to a local server shouldn't carry the exception.

## 4. Decision & Rationale for Corroboration

Option: the off-by-default marker block plus the `[ios] local_network` key, synced by `mobiler build` / `dev` through
`deep_links::toggle`, now shared and naming its block in errors.

**Mutation proof:** the decision was broken twice on purpose. The tests failed each time; the file was restored, and
the tests passed again.
- The template shipped the block switched on (`the_template_ships_local_networking_off` and the round-trip test
  panicked).
- The sync ignored the key (`on_switches_on_only_…` and `an_unknown_ios_key_…` panicked).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a self-hosted or dev-server app reaches its local server on iOS without a hand edit, and stays
  CLI-managed.
- **Negative:** the exception is in a release build too (ATS has no debug-only plist here). It covers local addresses
  only, which App Review accepts for local networking.
- **Negative:** whether iOS blocks plain `http://localhost` at all varies by version (ATS has exempted some local
  hosts); the option is harmless where it is redundant, and was not verified on a simulator (no Mac here).
- **Negative:** an app that already added its own `NSAppTransportSecurity` key and switches this on gets a duplicate
  key; remove the hand-added one first.
- **Negative:** an app scaffolded before CLI 0.73 must run `mobiler upgrade --apply` to get the markers; until then,
  `local_network = true` warns and changes nothing.
