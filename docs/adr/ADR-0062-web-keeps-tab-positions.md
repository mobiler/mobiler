# ADR-0062: The web shell keeps each visited tab's scroll position and shows a tab switch at once, with focus dropped; it keeps no screen, since it rebuilds the DOM on every update

Status:        Accepted
Date decided:  2026-10-20
Deciding PRs:  #307
Supersedes:    none
Code anchor:   mobiler-web/src/lib.rs (TabPositions, Pos, TabStep, tab_slot_of, current_pos, restore_pos, SCAFFOLD_DEPTH, TAB_POS; the Scaffold arm)
Conformance:   mobiler-web/src/lib.rs::a_switch_saves_the_old_tab_and_restores_the_new_one, mobiler-web/src/lib.rs::a_tab_whose_route_changed_while_away_starts_at_the_top, mobiler-web/src/lib.rs::push_and_pop_inside_a_tab_are_not_switches, mobiler-web/src/lib.rs::leaving_the_tabbed_screens_forgets_every_position

## 1. Context (The Problem)

ADR-0057 (Android) and ADR-0058 (iOS) keep each visited tab's screen, so returning to a tab finds it where it was
left, and a switch is instant. Both name the web shell as the one that still differs: every tab switch scrolls to the
top. The maintainer asked for the web to follow (2026-10-20).

The web shell can't keep a screen the way the native shells do. It renders the whole `Widget` tree into the DOM on
every core update: a Leptos signal over the root widget, with no live per-tab view to hold.

## 2. Hypothesis

What a user sees from ADR-0057/0058 is the tab's scroll position, the instant switch, and the keyboard closing. If the
root scaffold remembers each tab's scroll offset and restores it on return, then the web matches the native shells in
everything visible. A tab switch also drops the old tab's focus and skips the nav animation.

- A screen is tabbed when exactly one tab is selected (`tab_slot_of`, the native `tabSlotOf`).
- On a switch, the offset of the screen still on the page is saved under the old slot and its route. The page scroll
  is saved, and so is a `lazylist-fill`'s own scroll when the body is in fill mode. The new slot's saved offset is
  restored after the new DOM is in place, but only for the route it was saved with. A tab the app moved elsewhere
  while away starts at the top, never at another screen's offset (ADR-0057's "never stale").
- A push/pop inside one tab is not a switch: it animates as before.
- Leaving the tabbed screens forgets every offset, as the native shells drop their slots.
- Only the root scaffold keeps positions. A scaffold in a sheet, a Split or a body is counted out (`SCAFFOLD_DEPTH`).
- Nothing runs hidden: the web never holds a hidden screen, so ADR-0057's not-kept list has no web counterpart.

### 2.1. Refutation Conditions

- **Condition 1: a return restores the tab's position, for the same route only.** A move between tabbed and untabbed
  screens forgets them, and a push/pop isn't a switch.
  - **Validation Metric:** the four `TabPositions` tests named in `Conformance`.
- **Condition 2: it works in a browser.** Headless Chrome on barbershop's web client (CDP), 2026-10-20: Home scrolled
  to 600 and Services opened at 0. Services scrolled to 250. Back on Home, it was at 600; back on Services, at 250. The
  switch left no nav animation class on the body.
  - **Validation Metric:** the deciding PR's log.

## 3. Considered Options & Rationale for Refutation

- **Option A: keep each tab's DOM, hidden** `[reconstructed]`
  Rejected. The shell rebuilds the DOM on every update, so a kept subtree would be rebuilt anyway. Moving the shell to
  fine-grained reactive views is a rewrite far beyond this.
- **Option B: leave the web resetting to the top** `[recorded: ADR-0057 §5, ADR-0058 §5]`
  Rejected. The maintainer asked for the web to follow.

## 4. Decision & Rationale for Corroboration

Option: `TabPositions` in the root scaffold, as in §2. The logic is a pure struct, unit-tested on the host. The DOM
reads and writes are two small functions, `current_pos` and `restore_pos`.

**Mutation proof:** the logic was broken twice on purpose. The tests failed each time; the file was restored, and the
tests passed again.
- A return restored the top instead of the saved offset (`a_switch_saves_the_old_tab_and_restores_the_new_one`
  panicked: "the tab comes back where it was left").
- The route check was dropped, so any saved offset was restored
  (`a_tab_whose_route_changed_while_away_starts_at_the_top` failed).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the web, Android and iOS all return a user to where they were on a tab, and switch tabs at once.
- **Negative:** only the scroll position survives on the web, not the rest of a screen's state. The native shells
  also keep an open `Disclosure` or a field's cursor; the web rebuilds those on every update anyway.
- **Negative:** a restore happens one tick after the render (`setTimeout(0)`), so a slow device may show a frame at
  the top before it jumps.
