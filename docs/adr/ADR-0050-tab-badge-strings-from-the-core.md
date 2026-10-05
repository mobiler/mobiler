# ADR-0050: A tab's badge is two finished strings from the core — the drawn text (empty for a dot, the core's "99+" cap) and the spoken label — and every shell draws it in both the bar and the rail, adding no words of its own

Status:        Accepted
Date decided:  2026-10-05
Deciding PRs:  #286
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Tab.badge, TabBadge), mobiler-core/src/lib.rs (with_tab_dot, with_tab_count), mobiler-web/src/lib.rs (tab_badge), demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt (TabBadgeMark, TabIcon, TabLabel), demos/barbershop/iOS/Sources/Render.swift (tabBadgeMark, tabSpoken)
Conformance:   xtask/tests/adr_conformance.rs::adr_0050_every_tab_bar_and_rail_draws_the_badge, mobiler-core/src/lib.rs::with_tab_count_formats_and_caps, mobiler-core/src/lib.rs::with_tab_count_zero_clears_a_badge, mobiler-ui/src/lib.rs::tab_badge_round_trips

## 1. Context (The Problem)

The appointments team (Moj Termin admin, 2026-10-04) needed an unread mark on a bottom tab, seen from any tab. `Tab`
had only `label`, `selected`, `on_select` and `icon`, so they appended " •" to the label. That shifted the label and
was read aloud as "Nalog bullet". They suggested `enum TabBadge { Dot, Count(u32) }`, with each shell capping the
count at "99+".

Three existing rules met here. ADR-0033: the core formats numbers, and shells draw strings. ADR-0016: shells draw no
words of their own. ADR-0028: screen-reader text goes through the `A11y` wrapper, not per-widget fields. But a `Tab`
is not a `Widget`, so the wrapper cannot reach it.

## 2. Hypothesis

If `Tab` carries `badge: Option<TabBadge { text, label }>`, where both strings are finished by the core (`text`
empty for a dot, the count otherwise, capped at "99+" by `with_tab_count`; `label` the app's spoken words), and every
shell draws `text` and speaks `label` (else `text`) after the tab's name, in the bar and in the rail, then:
- the badge reads the same on every shell, in any language and with any plural rule, with no shell formatting;
- no layout can silently drop it.

### 2.1. Refutation Conditions

- **Condition 1 — the core finishes the strings.** 0 gives no badge, 1–99 the number, above 99 "99+".
  - **Validation Metric:** `with_tab_count_formats_and_caps`, `with_tab_count_zero_clears_a_badge`.
- **Condition 2 — the type crosses the wire.**
  - **Validation Metric:** `tab_badge_round_trips`.
- **Condition 3 — every shell draws it in the bar and the rail.**
  - **Validation Metric:** `adr_0050_every_tab_bar_and_rail_draws_the_badge`.
- **Condition 4 — the spoken rule is the same everywhere** (`label`, else `text`, after the tab's label). Review,
  plus the runtime checks in the deciding PR (web CDP, Android uiautomator).

## 3. Considered Options & Rationale for Refutation

- **Option A — `enum TabBadge { Dot, Count(u32) }`, shells format** `[recorded: docs/superpowers/specs/2026-10-05-tab-badge-design.md, Decision 1]`
  Rejected. Three shells would each format the number and apply the cap (against ADR-0033), and the spoken text would
  need a shell-side template.
- **Option B — a `ShellLabels` template such as `"{n} new"` for the spoken text** `[recorded: docs/superpowers/specs/2026-10-05-tab-badge-design.md, Decision 3]`
  Rejected. It can't express plurals ("1 nova poruka / 2 nove poruke / 5 novih poruka"), and the core has no plural
  rules.
- **Option C — finished `text` and `label` on the badge** `[recorded: the same spec, approved by the maintainer 2026-10-05]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. `label` is a screen-reader field on a non-widget. ADR-0028 governs widgets, which can be wrapped; a tab
can't, so this is a deliberate, scoped exception. ADR-0028 stays in force.

The Android dot is sized to 8 dp explicitly, because Material 3's content-less `Badge` is 6 dp; this makes the dot
match the web and iOS shells.

**Mutation proof:** the invariant was broken twice on purpose and failed each time (each file was then restored with
`git checkout -- <file>` and the test passed again).
- Kotlin: in barbershop's `MainActivity.kt`, the `NavigationRailItem`'s `icon = { TabIcon(t) }` was replaced with
  `icon = { t.icon?.let { Icon(iconFor(it), contentDescription = null) } }`. The test failed with
  `ADR-0050: demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt NavigationRailItem( doesn't draw the badge`.
- Swift: in barbershop's `Render.swift`, the `.accessibilityValue(tabSpoken(tab))` line in `navRail` was deleted. The
  test failed with `ADR-0050: demos/barbershop/iOS/Sources/Render.swift rail doesn't draw the badge`.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app passes its unread count straight through (`with_tab_count(tab, n, …)`, where 0 clears it),
  in its own language and plural forms.
- **Positive:** a new shell layout for tabs must route through the badge helper, or the conformance test fails.
- **Negative:** appending `Tab.badge` breaks full `Tab { .. }` literals (ADR-0008).
- **Negative:** the cap "99+" is fixed in the core, and an app cannot choose another without building `TabBadge`
  itself.
- **Negative:** the badge colours reuse the danger roles (`error_fill` / `on_error_fill`); there is no separate
  badge role, so an app can't colour badges unlike its Danger buttons.
- **Negative:** iOS draws its own badge (the tab bar is custom, not `UITabBarItem.badgeValue`), so it doesn't
  follow any future system badge style automatically.
