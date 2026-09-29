# ADR-0026: A live native view is controlled declaratively: the app owns its state as widget fields, one-shot commands are values the shell applies only when they change, and native events return as `Action::Input` keyed by the widget's `id`; there is no imperative handle

Status:        Accepted
Date decided:  2026-06-07
Deciding PRs:  #122, #123, #126, #127
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Widget::Video, Widget::Map), mobiler-core/src/lib.rs (video_player, video_playlist, map), template shells (iOS Render.swift VideoView + VideoSessionStore, MapWidgetView; Android MainActivity.kt VideoWidget, MapWidget), mobiler-web/src/lib.rs (the Video and Map arms)
Conformance:   mobiler-core/src/lib.rs::input_builders_carry_ids_values_and_event_tokens, mobiler-ui/src/lib.rs::widget_round_trips

## 1. Context (The Problem)

Until `Widget::Video`, every widget was a static description: the shell drew it and forgot it. A
video player is different. It holds native state (the decoder, the position, buffering) that must
survive while the core keeps re-rendering, and the app needs to steer it (play, pause, seek) and
hear from it (position, end).

The core's view is a pure function of the model, and every shell re-renders the whole tree on each
update (PR #127: iOS renders `core.view` whole). So the app can't hold a reference to "the player on screen". Any control had to
fit that model, or add new machinery to the ABI.

## 2. Hypothesis

If a live native view is steered only through ordinary widget fields and reports only through the
existing action path, then:

- no new ABI machinery is needed: no player handles, no command effects, no new `Action` variant;
- continuous state is app-owned, like `Toggle.value` (`playing`);
- a one-shot command is a value the shell acts on only when it changes (`seek_to_ms`,
  `seek_index`; `-1` means none), so re-rendering the same value repeats nothing;
- native events arrive as `Action::Input` under the widget's `id` or `"{id}.<event>"`, handled in
  `MobilerApp::input`, or as an `ActionToken` for a single typed event (`on_ended`);
- the shell, not the app, keeps the native object alive across re-renders.

### 2.1. Refutation Conditions

- **Condition 1 — control is carried in widget fields.** `video_player` builds a `Widget::Video`
  whose `id`, `playing` and `seek_to_ms` are the app's values, with `-1` as "no seek".
  - **Validation Metric:** `input_builders_carry_ids_values_and_event_tokens` asserts
    `video_player("v", …, false, -1, …)` yields `id == "v"`, `playing: false`, `seek_to_ms: -1`.
    It fails if these fields leave the widget. It would not catch a parallel imperative channel
    added beside them; review does.
- **Condition 2 — the control fields cross the wire.** `Widget::Video` with `playing`,
  `seek_to_ms`, `seek_index` and `Widget::Map` with its camera and markers round-trip.
  - **Validation Metric:** `widget_round_trips` (its `Video` and `Map` cases).
- **Condition 3 — the shell applies commands on change only.** Nothing mechanical. The shell code
  is Kotlin / Swift without a unit harness; review and the device checks in PRs #126 and #127.

## 3. Considered Options & Rationale for Refutation

- **Option A — imperative control (a player handle, or `cx` calls such as play / seek aimed at a
  widget)** `[reconstructed]`
  Not chosen. It would add an addressing scheme for on-screen views and new effect or ABI types,
  and the app's model would no longer describe what is playing.
- **Option B — declarative fields plus the existing `Action::Input` / `ActionToken` paths** `[recorded: commit 0926900 ("Controllable v1 (rides existing primitives — no new ABI machinery beyond the variant)"; "`playing` drives play/pause (app-owned, like Toggle.value)")]`
  Chosen.
- **Option C — level-triggered reconciliation (force the native player to match `playing` on every
  render)** `[recorded: PR #126 (the iOS VideoView "reconciled play/pause on **every** re-render (level-triggered)"); commit b6901e3 ("Seek/play/pause stay edge-triggered (apply only on change).")]`
  Shipped on iOS in #122 and replaced by edge-triggering in #126. The video card re-renders about once a second, so the
  shell paused a player the user had just started with its own controls.

## 4. Decision & Rationale for Corroboration

Option B. PR #122 shipped `Widget::Video` (mobiler-ui 0.17) with `playing`, `seek_to_ms` and
`on_ended`; the shell reports position about once a second as `Action::Input { id, Int(ms) }`.
PR #126 made iOS apply play/pause on change, as Android's `LaunchedEffect(playing)` already did.
PR #127 found that SwiftUI recreated the player view every tick, and moved the `AVPlayer` into a
`VideoSessionStore` keyed by `id`, so a recreated view rebinds to the same player. Android keeps the
`ExoPlayer` in `remember(url, urls)` and releases it on dispose.

Video v2 kept the pattern: it added `"{id}.duration"`, `"{id}.state"`, `"{id}.index"` and
`"{id}.buffered"` on the same `Input` path, and `seek_index` as a change-applied command.
`Widget::Map` (PR #144) follows it for events only (`"{id}.tap"`, `"{id}.marker"`). Its camera does
not: iOS re-applies `center_lat` / `center_lng` / `zoom` and the markers on every update (the
level-triggered Option C, so a user's pan snaps back), and Android applies them only when the view
is created, ignoring later changes (see §5). Continuous events from a *widget*
use this path; continuous events from a *plugin* use `cx.subscribe` (ADR-0006).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a new live view (video, map) needs one ABI variant and one arm per shell, nothing
  else. The app's model always states what the view should be doing.
- **Positive:** apps handle all native feedback in `MobilerApp::input`, the same place as slider
  and text input.
- **Negative:** `Widget::Map` breaks the pattern for its camera and markers, differently per shell
  (iOS level-triggered, Android set once). Found while writing this record; not yet fixed.
- **Negative:** a repeated command needs a changed value. Seeking to the same position twice means
  the app must pass `-1` in between (the shells reset their "last seek" on `-1`).
- **Negative:** events are stringly keyed (`"{id}.state"`) with untyped values. A typo in the id
  suffix is silently ignored.
- **Negative:** the shell must keep the native object alive by itself, and each shell does it
  differently (iOS by `id`, Android by composition position and `url`). The iOS sessions are never
  evicted: PR #127, "Known v1 tradeoff: sessions are cached by id and not evicted".
- **Negative:** the web shell rebuilds the whole tree on each update and cannot keep a `<video>`
  alive, so app-driven play, seek and position events are not supported there. PR #122:
  "app-driven play/seek/position are iOS/Android only — the web shell rebuilds the whole tree each
  update".
