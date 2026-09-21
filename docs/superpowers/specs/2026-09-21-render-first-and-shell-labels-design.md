# Render first, and app-supplied shell labels — design

**Date:** 2026-09-21. **Base:** `main` @ `31742b2` (ui 0.24.0 / core 0.35.0 / web 0.35.0, CLI 0.52.0).
**Requests:** `docs/render-before-requests.md`, `docs/confirm-dialog-labels.md` (appointments admin app).
**Constraint:** the app will not patch generated shell code. Everything must arrive via a release +
`mobiler upgrade`.

Two releases. Release 1 is urgent (text input that starts a request is broken on Android) and
changes no API or ABI. Release 2 is additive API + one widget-field ABI change.

---

## Release 1 — render before the requests finish (core 0.35.1 + CLI 0.52.1)

### Findings (verified against the code)

- `mobiler-core/src/lib.rs:403-418` (`MobilerShell::update`) emits notifications, requests,
  streams, then `render()` **last**.
- Android `Core.kt:434-463` (`process`) walks the effects in order and, for `Effect.Plugin`,
  **awaits** `dispatch` and the recursive `process(core.resolve(...))` inline. So the batch's
  `Render` (and any notify queued after a request) waits for a full network round trip.
- iOS `Core.swift:68-78` already spawns a `Task` per `.plugin`; web `mobiler-web/src/lib.rs:226-228`
  already `spawn_local`s. Neither reproduces the bug.

### Changes

1. **Core — render first.** In `MobilerShell::update`, push `render()` before notifications,
   requests and streams. The model is final when `app.update` returns, so the first frame reflects
   it; replies re-render through their own `Fired` update. The relative order of notifications,
   requests and streams is unchanged.
   - Test: an update that makes a request, a notify and a subscribe yields `Render` as the first
     effect, and the remaining effects keep their current relative order.
2. **Android shell — never await a request inline.** In `Core.kt` `process`, the `Effect.Plugin`
   branch becomes `viewModelScope.launch { val resp = dispatch(...); process(core.resolve(...)) }`,
   matching iOS/web. Requests still *start* in batch order; they no longer serialise behind each
   other, and later effects in the batch apply immediately. `PluginNotify` stays inline (it is
   fire-and-forget and already fast; keeping it inline preserves notify ordering, e.g.
   `stream/unsubscribe` before a re-subscribe in the same batch).
3. **Android shell — controlled text fields keep their own state.** `Widget.TextField` and
   `Widget.SearchField` (`MainActivity.kt:1041`, `:1055`) hold a remembered `TextFieldValue`
   keyed by `widget.id`, plus the last text the field reported. On recomposition, adopt
   `widget.value` only when it differs from that last-reported text (i.e. the app changed the
   value itself — clear, autofill, formatting); otherwise keep the local value, cursor and
   selection. A late or duplicate render can then never rewind the field or move the cursor.
   - iOS (SwiftUI `TextField` binding) and web are unaffected by the timing bug; not changed.

### Acceptance

- Android emulator, an `input()` that fires `GET /customers?q=…`: `adb input text Nikola` leaves
  `Nikola` in the field; the last request carries `q=Nikola`; one request per keystroke.
- A button that sets "Saving…" and starts a request shows "Saving…" before the reply, with a
  delayed endpoint (≥2 s).
- All demos build and behave as before (CI matrix + barbershop spot check on Android).

### Release shape

CLI + core patch: bump `mobiler-core` 0.35.1 (template `Cargo.toml.tmpl` string too), refresh
lockfiles, publish core, then tag CLI `v0.52.1`. `mobiler-ui`/`mobiler-web` unchanged (web's
dependency requirement on core 0.35 already accepts 0.35.1). The app picks the fix up with
`mobiler upgrade` (Core.kt/MainActivity.kt 3-way merge) + `cargo update -p mobiler-core`.

---

## Release 2 — labels the app controls (ui 0.25 / core 0.36 / web 0.36 + CLI 0.53)

### 2a. Confirm dialog labels + destructive style

**Core API (additive).** A small builder, not a 6-argument function:

```rust
pub struct Confirm { /* title, message, confirm_label, cancel_label, destructive */ }
impl Confirm {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self;
    pub fn confirm_label(self, label: impl Into<String>) -> Self;
    pub fn cancel_label(self, label: impl Into<String>) -> Self;
    pub fn destructive(self) -> Self;
}
impl<E> Cx<E> {
    pub fn confirm_with(&mut self, dialog: Confirm, then: impl FnOnce(PluginResponse) -> E + Send + 'static);
}
```

`confirm(title, message, then)` stays and becomes `confirm_with(Confirm::new(title, message), then)`.
Wire format: the same `dialog`/`confirm` op; input JSON
`{"title","message","confirm_label"?,"cancel_label"?,"destructive"?}`. Absent fields are omitted
(`skip_serializing_if`), so a default `confirm` serialises byte-identically to today. Shells treat
missing labels as `OK` / `Cancel` and missing `destructive` as `false`. Response unchanged:
`ok: true` on confirm; `ok: false` on cancel, back, or tap-outside.

**Android — move the dialog into Compose.** Today `DialogPlugin` builds an `android.app.AlertDialog`
that cannot see the Compose theme (no `colorScheme.error`, no Large-density sizing). Instead,
follow the existing `PhotoPicker` bridge pattern: `DialogPlugin` publishes a pending request
(title, message, labels, destructive, a completion callback) to a small `ConfirmHost` object and
suspends; `MainActivity`'s composition renders a Material 3 `AlertDialog` while one is pending.
- Confirm button: `TextButton` (or `FilledTonalButton` in the danger tone when `destructive`)
  labelled `confirm_label`; colour `colorScheme.error` when `destructive`.
- Dismiss button labelled `cancel_label`; `onDismissRequest` (back / outside) → `ok: false`.
- Both buttons use the same min height as other buttons at the current `Density` (56dp at Large).
- Coroutine cancellation dismisses the dialog (as today).

**iOS.** `UIAlertAction(title: cancel_label, style: .cancel)` and
`UIAlertAction(title: confirm_label, style: destructive ? .destructive : .default)`. System
alerts cannot be resized; their 44pt buttons already meet the HIG target. This is documented,
not worked around.

**Web — replace `window.confirm` with a DOM modal.** `window.confirm` cannot be relabelled or
styled. A small modal in `mobiler-web`: backdrop + card with title, message and two buttons,
styled from the existing theme CSS variables (danger colour for a destructive confirm), Large
density respected. Escape and backdrop click → `ok: false`; Enter → the focused button; focus
starts on the dismiss button when `destructive`, otherwise on the confirm button. Resolves through
a oneshot channel, the same way `take_datetime` does.

### 2b. Date / time picker labels

**Core API (additive).**

```rust
pub struct Picker { /* title, confirm_label, cancel_label — all optional */ }
impl Picker {
    pub fn new() -> Self;
    pub fn title(self, t: impl Into<String>) -> Self;
    pub fn confirm_label(self, l: impl Into<String>) -> Self;
    pub fn cancel_label(self, l: impl Into<String>) -> Self;
}
impl<E> Cx<E> {
    pub fn pick_date_with(&mut self, picker: Picker, then: ...);
    pub fn pick_time_with(&mut self, picker: Picker, then: ...);
}
```

`pick_date`/`pick_time` keep sending an empty input `""`. The `_with` variants send
`{"title"?,"confirm_label"?,"cancel_label"?}`. Shells accept both `""` and JSON; missing fields
keep today's text.
- **Android:** keep the native `DatePickerDialog` / `TimePickerDialog`; apply
  `setButton(BUTTON_POSITIVE, …)`, `setButton(BUTTON_NEGATIVE, …)` and `setTitle` when supplied.
  Month/day names still follow the device locale (platform behaviour, documented).
- **iOS:** the action sheet title (`"Pick a date"`/`"Pick a time"`), `"Cancel"` and `"Done"` come
  from the input when supplied.
- **Web:** the browser's native picker draws its own chrome in the browser's language; labels
  are accepted and ignored. Documented as a platform limitation.

### 2c. LazyList end label

- **ABI:** add `end_label: Option<String>` to `Widget::LazyList` (mobiler-ui 0.25). The
  Kotlin/Swift/web generated types regenerate; all three shells updated in the same release.
- **Behaviour change:** `None` renders **nothing** at the end of an exhausted paged list (today:
  hard-coded `"End of list"` in Android `MainActivity.kt:979`, iOS `Render.swift:884`, web
  `lib.rs:1515`). `Some(text)` renders that text in the current style.
- **Builder:** `with_end_label(widget, label)` in core, following the `with_refresh` wrapper
  pattern (the list builders are free functions returning `Widget`). `lazy_list` and
  `lazy_list_static` set `end_label: None`.
- **Demos:** barbershop (the only `lazy_list` user) passes its own text so its screenshot stays the
  same.

### Out of scope (deliberately)

- **`LazyList { fill: true }`** (list fills the remaining body instead of the 480dp cap). A
  lazy list can't sit at unbounded height inside a scrolling body, so "fill" means the body must
  stop scrolling when such a list is present. That is a layout change with its own design; tracked
  separately.
- Other hard-coded shell strings not reported by the app. Audit them in the Release 2 whole-branch
  review and list any left for later; don't widen scope silently.

### Acceptance

1. The cancel-booking dialog shows **"Otkaži termin"** in the danger colour and **"Ne, vrati se"**
   (Android, iOS, web). Confirm → `ok: true`; dismiss, back or outside → `ok: false`.
2. `confirm` without the new fields still shows `OK` / `Cancel`; demos unchanged.
3. Android dialog buttons meet the Large-density height; iOS documented as system-sized.
4. `pick_date_with(Picker::new().title("Izaberi datum").confirm_label("Izaberi").cancel_label("Otkaži"))`
   shows those strings on Android and iOS; `pick_date` unchanged.
5. An exhausted paged list with no `end_label` shows nothing at the end; with
   `with_end_label(…, "Kraj liste")` it shows that text; barbershop looks the same as before.
6. Core unit tests: `confirm` input JSON byte-identical to today; `confirm_with` / `_with` pickers
   serialise the optional fields; `LazyList` round-trips with `end_label`.

### Release shape

Libraries in dependency order (ui 0.25 → core 0.36 → web 0.36) via the release-libs flow, then
CLI 0.53 (templates: Android Core.kt + MainActivity.kt, iOS Core.swift + Render.swift, template
`Cargo.toml.tmpl` string). Whole-branch review across all three shells before merge (cross-shell
divergence lesson). README/capability docs updated for `confirm_with`, `pick_*_with`,
`with_end_label` before the lib publish.
