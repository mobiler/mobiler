# A badge (dot or count) on a tab

**Asked by:** the appointments team (Moj Termin admin app), 2026-10-04, on CLI 0.65.3 / core 0.43. The salon owner
wants an unread dot on the "Nalog" tab for support replies, visible from any tab and cleared once read. Today they
append " •" to the label, which shifts it and is read aloud as "Nalog bullet".
**Release:** ui 0.31 / core 0.44 / web 0.44, then CLI 0.66.
**Constrained by:**
- ADR-0007: `tab` / `tab_icon` keep their signatures and leave the new field at its no-op default; the badge arrives
  through new `with_*` functions.
- ADR-0008: `Tab.badge` is appended. That is BREAKING for `Tab { .. }` literals and is announced as such, with a
  minor bump.
- ADR-0009: libraries first, then the template shells in the CLI PR.
- ADR-0015: barbershop's shells first, then the template is ported from barbershop's diff.
- ADR-0016: every string a shell draws comes from the app. The badge's text and its spoken label are both app
  values; the shells add no words.
- ADR-0019: the badge's colours are optional palette roles; an app that sets none gets the platform default.
- ADR-0028: screen-reader text goes through the `A11y` wrapper, not per-widget fields. A tab is not a widget and
  can't be wrapped, so the badge carries its own spoken label (see Decision 3).
- ADR-0033: the core formats numbers. The core turns a count into the drawn string, "99+" included.

**New record:** ADR-0050, a tab badge's text and spoken label are finished strings from the core.

## Decisions (approved 2026-10-05)

### 1. Wire type (`mobiler-ui`)

```rust
pub struct Tab {
    pub label: String,
    pub selected: bool,
    pub on_select: ActionToken,
    pub icon: Option<Icon>,
    /// BREAKING (tab badge release): `Tab { .. }` literals need this field; `tab` / `tab_icon` set `None`.
    pub badge: Option<TabBadge>,
}

pub struct TabBadge {
    /// Drawn as-is inside the badge. Empty = a small dot with no text.
    pub text: String,
    /// What a screen reader says for the badge, e.g. "3 nove poruke". Read after the tab's label.
    pub label: String,
}
```

A string rather than the suggested `enum TabBadge { Dot, Count(u32) }`: with a `u32`, each shell would format the
number and apply the cap itself (ADR-0033), three times.

### 2. Builders (`mobiler-core`)

They follow the `with_icon(widget, icon)` shape:

```rust
pub fn with_tab_dot(tab: Tab, label: impl Into<String>) -> Tab
pub fn with_tab_count(tab: Tab, count: u32, label: impl Into<String>) -> Tab
```

- `with_tab_dot` sets `TabBadge { text: "", label }`.
- `with_tab_count` with `count == 0` sets `badge: None`, so an app passes its unread count straight through. With
  `1..=99` the text is the number; above 99 it is `"99+"`.
- Calling either again replaces the badge.
- `tab` and `tab_icon` are unchanged and leave `badge: None`.

### 3. The spoken label

The app passes it with the badge. A `ShellLabels` template such as `"{n} new"` can't express plurals ("1 nova
poruka / 2 nove poruke / 5 novih poruka"), and the core has no plural rules (ADR-0033 §5). It is a screen-reader
field on a non-widget, which ADR-0028 doesn't cover; ADR-0050 records this as a deliberate exception, and ADR-0028
stays in force for widgets.

An empty `label` is allowed. The badge is then silent for a dot, and reads its `text` for a count.

### 4. How each shell draws it

The bottom bar and the wide-screen rail both show the badge, on Android and iOS.

| | Has an icon | No icon (label-only tab) | Spoken |
|---|---|---|---|
| Android | `BadgedBox` on the `NavigationBarItem` / `NavigationRailItem` icon: `Badge()` for a dot, `Badge { Text(text) }` for a count | the badge after the label, in a `Row` | the item's semantics gain `label`, so TalkBack reads "Nalog, 3 nove poruke" |
| iOS | `.overlay(alignment: .topTrailing)` on the icon: an 8 pt circle, or a capsule with the text | the badge after the label, in an `HStack` | `.accessibilityValue(label)` on the tab's button |
| Web | `span.tab-badge` over the icon's top-right | the span after the label | visible badge `aria-hidden`; `label` in a visually hidden span inside the button |

**Colours:** the fill is `palette.error_fill` and the text is `palette.on_error_fill`. When unset, the platform
default applies: M3's `BadgeDefaults` on Android, `.red` with white text on iOS, and a CSS default red with white
text on the web.

**Density:** under `Density::Large` the count's font follows the tab label's larger size. The dot keeps its size.

## Out of scope

- Badges anywhere else: the top bar, list rows, the FAB. A `Widget::Badge` already exists for in-body labels.
- Animation when the badge appears or changes.
- An OS app-icon badge count (the launcher / home-screen number). That is a push or plugin concern.

## Testing

- `mobiler-ui`: a round-trip test for a `Tab` with each kind of badge (ADR-0008 condition 1).
- `mobiler-core`: `with_tab_count` gives no badge for 0, `"7"` for 7, `"99+"` for 100; `with_tab_dot` sets empty
  text; `tab` / `tab_icon` leave `None`.
- `xtask/tests/adr_conformance.rs`: an ADR-0050 test that the reference shell (barbershop) and the template, on
  Android and iOS, and `mobiler-web` read the tab's badge in both the bar and the rail. Its mutation proof (delete
  the badge read from one shell, see it fail) goes in ADR-0050 §4.
- Runtime:
  - barbershop on web via headless Chrome/CDP: the dot, a count, "99+", and the accessible name;
  - barbershop on the Android emulator: the bar on a phone, the rail on a wide screen, and the TalkBack description
    from a uiautomator dump;
  - iOS by a TestFlight build the maintainer checks on a phone.
- The barbershop demo shows a dot on one tab and a count on another, so every build exercises both.

## Release

1. **Libraries PR:** `Tab.badge` + `TabBadge` (ui), the builders (core), the web shell arm (web), barbershop's
   Android and iOS shells and its app, ADR-0050, and README updates (`mobiler-ui`, `mobiler-core`, root, and the
   Upgrading BREAKING note). Publish ui 0.31 → core 0.44 → web 0.44, after the maintainer confirms.
2. **CLI PR:** port the barbershop shell diff into the template, bump the template's library versions, add the
   `capabilities.json` / README entries, and release CLI 0.66.
3. **Afterwards:** tell the appointments team to replace " •" with `with_tab_count(tab, unread, ...)`. Delete
   `docs/tab-badge.md` once they confirm.
