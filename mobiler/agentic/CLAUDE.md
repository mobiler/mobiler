# CLAUDE.md — building this app with Mobiler

This project is a **[Mobiler](https://github.com/mobiler/mobiler)** **mobile** app: you write
the app's **logic AND UI in Rust**, and a generic prebuilt shell renders it natively on
**Android (Jetpack Compose)** and **iOS (SwiftUI)** — no per-app Kotlin/Swift, no per-app UI
codegen. (Mobiler builds on [Crux](https://github.com/redbadger/crux): a Rust core + a fixed
`Widget` wire ABI.)

It's **backend-agnostic**: talk to whatever HTTP API you already have (via `cx`), or store the
app's data on the device — nothing here assumes a particular server.

## The model you implement

Everything lives in `shared/src/app.rs` as a `MobilerApp`:

```rust
impl MobilerApp for MyApp {
    type Event = Msg;     // your typed events — a `#[derive(Serialize, Deserialize)]` enum
    type Model = Model;   // your state (keep it serde-serializable)

    // Handle an event: mutate the model and/or request effects via `cx`.
    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) { … }

    // Text fields / toggles / sliders report here (id + InputValue::{Text,Bool,Int}).
    fn input(&self, id: &str, value: InputValue, model: &mut Model, cx: &mut Cx<Msg>) { … }

    // Build the UI as a Widget tree from the builders below.
    fn view(&self, model: &Model) -> Widget { … }

    fn restore(&self, data: &str, model: &mut Model) { … }   // rehydrate persisted state
    fn init(&self, model: &mut Model, cx: &mut Cx<Msg>) { … } // first effects, e.g. cx.get(...)
}
pub type App = MobilerShell<MyApp>;   // this is what the shells render
```

## The widget vocabulary (this is the whole UI toolkit — do not invent widgets)

Build the `view` from these builders (`use mobiler_core::*;`):

- **layout**: `column`, `row`, `grid`, `card` / `card_button`, `stack` (z-stack/overlay),
  `spacer(Spacing::…)`, `divider`
- **content**: `text` / `title` / `subtitle` / `caption` / `emphasis`,
  `image(src, ImageShape::…, ImageRatio::…)`, `badge(label, Tone::…)`, `color_dot(ProjectColor::…)`
- **input / actions**: `button(label, ButtonStyle::…, Msg)`, `icon_button(Icon::…, Msg)`,
  `chip(label, selected, Msg)`, `text_field(id, placeholder, value)`,
  `toggle(id, label, value)`, `checkbox(id, label, value)`, `slider(id, value, max)`,
  `stepper(value, dec_msg, inc_msg)`
- **shell**: `scaffold(title, dark_mode, tabs, body)`, `scaffold_back(…, back_msg)`,
  `nav_scaffold(title, dark_mode, tabs, body, &model.nav, back_msg)` — top bar + bottom
  tabs + scrollable body; `tab(label, selected, Msg)`

Style is **intent**, not pixels: pass enums (`ButtonStyle`, `CardStyle`, `Tone`, `TextStyle`,
`ImageShape`, `ImageRatio`, `Spacing`, `ProjectColor`, `Icon`) — each shell maps them to its
native look (Material 3 / SwiftUI / a CSS theme). If a design needs something outside this
set, pick the closest builder and note the gap; **do not try to add new widgets** (that's an
ABI change in the framework, not app work).

## Navigation, capabilities, theme

- **Navigation**: hold a `Nav<Route>` in your `Model` (`push` / `pop` / `reset` / `current` /
  `depth`); render with `nav_scaffold(...)`. The core owns the stack; the shell animates it.
- **Capabilities** = device/platform APIs as async effects via `cx`, fulfilled natively:
  - fire-and-forget: `cx.toast`, `cx.copy`, `cx.share`, `cx.open_url`, `cx.haptic`, `cx.save`
  - request/response (take a `then: |resp| -> Msg` closure): `cx.get` / `cx.post` / `cx.patch`
    / `cx.delete` (HTTP/JSON), `cx.confirm`, `cx.pick_photo`, `cx.capture_photo`, `cx.device_model`
  - **Data**: call whatever HTTP API you have with `cx.get` / `post` / `patch` / `delete`, **or**
    keep it on-device with `cx.save` + `restore`. Never open a database or socket directly from the app.
- **Theme is data**: set `dark_mode` on the `Scaffold`; the shell themes the whole app from it.

## Golden rules

1. All state, logic, **and** UI live in the Rust core (`shared/`). You never edit `Android/` or
   `iOS/` (they're the generic shells) and never hand-write native UI.
2. Keep the `Model` serde-serializable (so `cx.save`/`restore` and the wire ABI work).
3. Design within the widget vocabulary above.
4. Data comes from a backend over `cx.http` (JSON), or from local `cx.save`/`restore`.
5. Prefer small, typed `Msg` variants; put real logic in `update`, not in `view`.

## Build & run

```bash
cargo test                         # run the Rust core's tests (do this often)
mobiler dev                        # build core → gen types → build APK → install + launch (Android)
mobiler build android              # build the APK only
bash iOS/build-ios.sh              # build for the iOS simulator (on a Mac; needs Xcode + XcodeGen)
```

## Where to look

- API docs: <https://docs.rs/mobiler-core>
- Worked examples (study these): the `demos/` in the Mobiler repo — `coffee` (storefront,
  images, grid) and `todo` (tabs, nav, CRUD, on-device persistence).
