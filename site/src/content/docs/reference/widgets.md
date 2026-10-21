---
title: Widgets
description: The widget vocabulary a Mobiler core builds its screens from, rendered natively on Android, iOS and the web.
---

Your `view` returns a `Widget` tree built from these functions (all in `mobiler_core`). This page lists
the main builders. Each shell renders
every widget: Compose on Android, SwiftUI on iOS, the DOM on the web. The full signatures are on
[docs.rs/mobiler-core](https://docs.rs/mobiler-core).

## Layout

| Builder | What it is |
|---|---|
| `column`, `row`, `stack` | Vertical, horizontal and layered layout. |
| `grid`, `with_columns` | A grid of cells. |
| `card`, `card_button`, `with_long_press` | A card with a style; a tappable card; press-and-hold for a second action. |
| `split` | Master and detail side by side on wide screens. |
| `scroller`, `scroller_hinted` | A horizontal scroller; the hinted one fades its trailing edge. |
| `lazy_list`, `lazy_list_static` | A paged list with infinite scroll and pull-to-refresh; `with_fill` makes it fill the screen's remaining height, `with_end_label` names its end. |
| `spacer`, `divider` | Space and separators. |

## Text

`title`, `headline`, `display`, `subtitle`, `text`, `caption`, `emphasis`, `styled`, `badge`, `step_text`.

## Input

| Builder | What it is |
|---|---|
| `button`, `button_with`, `icon_button` | Filled, outlined, text and tonal buttons; with a tone, a leading icon or full width; icon-only buttons. |
| `text_field`, `field`, `secure_field`, `email_field`, `phone_field`, `number_field`, `decimal_field`, `url_field`, `multiline_field`, `search_field` | Text input, each with the right keyboard; `with_error` shows a validation message. |
| `checkbox`, `toggle`, `chip`, `slider`, `stepper`, `segmented`, `rating` | Choices and values. |
| `calendar`, `calendar_in` | An inline month calendar, localized, with busy dots per day. |
| `swipe_action` | A row with actions revealed by a swipe. |
| `steps` | A step indicator. |

Inputs report their value to your core's `input(id, value, …)`.

## Navigation

| Builder | What it is |
|---|---|
| `scaffold`, `scaffold_back`, `nav_scaffold` | A screen with a top bar and bottom tabs, adaptive on tablets (a side rail); the other two add the back arrow and system back for a pushed screen, `nav_scaffold` driven by `Nav`. |
| `tab`, `tab_icon` | A bottom tab, with an optional icon. |
| `with_fab`, `with_sheet`, `with_refresh` | A floating action button, a bottom sheet, pull-to-refresh. |
| `with_tab_dot`, `with_tab_count` | A badge on a tab, read aloud after its name. |
| `with_bottom_bar` | A bar pinned above the tabs or the keyboard. |
| `Nav` (`new`, `push`, `pop`, `current`) | The navigation stack in your model. |

## Media

`image`, `avatar` (`with_initials`, `avatar_status`), `color_dot`, `progress`, `skeleton`, `pdf_view`,
`web_view`, `video_player` and `video_playlist` (with poster, captions, rate, volume, a playlist and picture-in-picture), and
`map` with `marker`s (MapKit on iOS, MapLibre on Android and the web, no API key).

## Charts

`chart` in eight styles (`bar_chart`, `line_chart`, `stacked_bar_chart`, `pct_stacked_bar_chart`,
`pie_chart`, `donut_chart`, `rings_chart`, `gauge_chart`), with an optional axis and legend, and
`region_chart` for value bands over time.

```rust
chart(
    vec![ChartSeries::new("Alex", vec![1.0, 2.0, 3.0]),
         ChartSeries::new("Sam",  vec![1.0, 1.0, 2.0])],
    vec!["Mon".into(), "Tue".into(), "Wed".into()],
    ChartStyle::StackedBar, /* axis */ true, /* legend */ true,
)
```

## Accessibility

Wrap any widget with `a11y(child, label)` (plus `with_a11y_hint` and `with_a11y_role`) to name it for
VoiceOver and TalkBack, and as an `aria-label` on the web. Text follows the system font size on every
platform.

## Look

`with_theme`, `with_appearance`, `with_labels` and `with_ios_glass_tabs` apply to a scaffold. See
[Theming](/guides/theming/).
