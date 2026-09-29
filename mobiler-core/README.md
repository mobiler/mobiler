# mobiler-core

> Mobiler's runtime — the developer-facing API.

Implement **`MobilerApp`** with your typed events, model, and a `view` built from the
widget [builders](https://docs.rs/mobiler-core). Mobiler wraps it in **`MobilerShell`**,
a [Crux](https://github.com/redbadger/crux) app speaking the fixed UI ABI
([`mobiler-ui`](https://crates.io/crates/mobiler-ui)) — so the native shell stays
generic and you never touch the wire protocol.

```rust
impl MobilerApp for Counter {
    type Event = Msg;
    type Model = Model;

    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Increment => model.count += 1,
            Msg::Greet => cx.notify("toast", "show", "Hi from Rust!"),
        }
    }

    fn view(&self, model: &Model) -> Widget {
        column(vec![
            title("Counter"),
            text(format!("count: {}", model.count)),
            button("Increment", ButtonStyle::Filled, Msg::Increment),
        ])
    }
}

pub type App = MobilerShell<Counter>;
```

- **Capabilities** via `Cx` — device APIs as async effects, reached through typed
  helpers in `update`/`input`. Built in:
  <!-- capabilities:start format=inline (generated from capabilities.json — run `cargo run -p xtask -- gen-readme`) -->
  HTTP, storage, clipboard, share, browser, toast, snackbar, device info, the app's version/build, the OS light/dark setting, haptics, a confirm dialog, the photo picker, camera capture, the date picker, and the time picker.
  <!-- capabilities:end -->
  Each is an opaque `{plugin, op, input}` effect, so adding one never changes the wire
  ABI; the generic shell fulfils them natively on Android, iOS, and the web.
- **Navigation** — a core-owned `Nav<Route>` stack + `nav_scaffold`.
- **Theme-as-data** — dark mode, brand color, corner radius, and a `Density`
  (Compact/Comfortable/Large — `Large` scales up control sizing and spacing for touch)
  all flow through the `Widget` tree, plus a design system's tokens: a light/dark `Palette` of
  colour roles, `with_appearance` (Light/Dark/System), custom fonts (`FontFamily::Custom` +
  `mobiler fonts sync`), a per-style `TypeScale`, and per-component corner `Shapes`.
- **Feedback & actions** — `cx.snackbar` (with an optional Undo), an extended FAB
  (`with_extended_fab`), a pinned bottom bar for a screen's main actions (`with_bottom_bar`),
  a step indicator (`steps` / `with_step_caption`), status badges with icons (`with_icon`),
  initials avatars (`with_initials`), fixed grid columns (`with_columns`), dashed cards, and
  `cx.open_url_then` (did the link open?).
- **Charts** — typed builders for data viz: `bar_chart`/`line_chart`, multi-series
  `chart`/`stacked_bar_chart`/`pct_stacked_bar_chart`, `pie_chart`/`donut_chart`, fitness-style
  `rings_chart`, a `gauge_chart`, and `region_chart` (variable-width coverage-gap bands).
- **Live native views** — builders for native-engine widgets the shell hosts: `video_player`
  (AVPlayer / ExoPlayer / `<video>`), `web_view`, `pdf_view`, and `map` (MapKit / MapLibre —
  markers + tap events, no API key).
- **Accessibility** — `a11y(child, label)` (+ `with_a11y_hint` / `with_a11y_role`) names any
  widget for VoiceOver / TalkBack.
- **Localization** — `format` (synchronous, ICU-free locale-aware currency / number / date
  formatting via `Locale` + `Currency`, plus `weekday_short` / `month_year` and
  `Locale::week_start` for a localized `calendar_in`) and `i18n` (`negotiate` a device
  language + a tiny fallback-aware `Catalog` for translating UI strings in `view`).

Most users go through the [`mobiler`](https://crates.io/crates/mobiler) CLI, which
scaffolds a project wired to this crate and a generic native shell.

## Upgrading

**Breaking in mobiler-ui 0.29 / mobiler-core 0.40** (the design release — additive in spirit,
but new fields on existing types):

- `Theme` gained `palette`, `type_scale` and `shapes`: write `Theme { seed, ..Default::default() }`
  instead of listing every field.
- New fields on existing widgets break full literals and exhaustive patterns: `Scaffold`
  (`appearance`, `bottom_bar`), `Fab.label`, `Badge.icon`, `Avatar.{initials, size}`,
  `Grid.columns`; new variants `TextStyle::{Display, Headline}`, `FontFamily::Custom`,
  `CardStyle::Dashed`, `Icon::DoneAll`, `Widget::Steps`. Build widgets through the builders
  (`scaffold`, `with_fab`, `badge`, `avatar`, `grid`, …) and match with `..`.
- Custom native shells must handle the new cases; `mobiler upgrade --apply` updates generated ones.

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
