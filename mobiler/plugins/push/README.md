# push — remote push notifications, APNs/FCM (free, bundled)

> ⚠️ **Experimental — not yet end-to-end device-tested.** The iOS *receive* path (notification →
> the events stream → your `update`) is verified on the simulator, and everything compiles + installs
> on both platforms. The real-APNs/FCM **token round-trip** (`register` → a server push from
> Apple/Google) has **not** been validated on a physical device yet. Treat the API as stable-ish but
> the delivery path as unproven; please report what you find. (Mobiler itself is experimental — see
> the project README.)

```bash
mobiler plugin add push
```

Server-sent push for the native shells — the companion to the local `notifications` plugin. Two
surfaces, both riding existing Mobiler primitives (no ABI change):

```rust
// 1) Get the device token (one-shot) → POST it (with your tenant) to your backend.
cx.plugin("push", "register", "", Msg::PushToken),
Msg::PushToken(r) => if r.ok { /* r.as_text() = {"token":"…","platform":"apns"|"fcm"} */ },

// 2) Subscribe to inbound pushes (the streaming primitive) — do this at startup so a tap that
//    launched the app (buffered by the shell) isn't missed.
cx.subscribe("push", "push", "events", "", Msg::PushEvent),
Msg::PushEvent(r) => if r.ok {
    // r.as_text() = the notification's JSON payload + "mobiler_push": "opened" | "received",
    //            or {"type":"token_refresh","token":"…"} when the OS rotates the token.
},
```

## Event kinds: `opened` vs `received`

Every notification event is the push's payload object (its data keys, plus `title`/`body` when the
message has them) with one reserved key, **`mobiler_push`**, saying what happened:

```json
{"booking_id":"01J…","title":"New booking","body":"…","mobiler_push":"opened"}
{"booking_id":"01J…","title":"New booking","body":"…","mobiler_push":"received"}
{"type":"token_refresh","token":"…"}
```

| `mobiler_push` | Meaning | Buffered until you subscribe? |
|---|---|---|
| `"opened"` | The user **tapped** the notification (app running, backgrounded, or launched by the tap). | **Yes**: a tap that launched the app arrives right after startup. |
| `"received"` | The push arrived **while the app was in the foreground**. | **No**: live only. A push that arrives while the app is backgrounded or dead produces no event; the tray notification is the record, and tapping it produces `opened`. |
| *(absent)* | `{"type":"token_refresh",…}`: re-POST the token. | Yes |

**Navigate only on `opened`.** On `received`, refresh in place (reload the list, bump a badge). Don't
jump the user to another screen mid-task. Opening the app from the launcher instead of the
notification produces **no** event, so it never lands on a stale screen. The meaning is the same on
iOS and Android. Apps that ignore the key see payloads shaped exactly as before.

Taps only are buffered (bounded at 32, oldest dropped) and flushed on subscribe. Subscribe at startup.

- **`register`** asks for notification authorization, registers with APNs/FCM, and returns the device
  token. **`events`** (subscribe) delivers `opened`/`received` notification events and token
  rotations; `cx.unsubscribe("push")` stops it.
- **iOS** = native **APNs** via system frameworks (no third-party SDK). The shell's `AppDelegate`
  receives the token + notification callbacks and forwards them to `PushBridge`; this plugin adapts
  them to the cx ABI: `willPresent` → `received`, a tap (`didReceive`, default action) → `opened`.
- **Android** = **Firebase Cloud Messaging** (the only way to get a device token on stock Android).
  A `FirebaseMessagingService` posts the tray notification for each **data** message, with a tap
  intent carrying that notification's own payload (→ `opened`), and emits `received` when the app is
  in the foreground. **Notification** messages (a `notification` block) sent while the app is
  backgrounded are displayed by FCM itself: their tap still produces `opened` with the data keys, but
  without `title`/`body`, and no custom icon/channel applies. Prefer **data** messages.
- **Web** = graceful no-op (`register` returns `ok:false`); browser web-push is a separate concern.

## Two manual setup steps `plugin add` can't do

1. **Android — `google-services.json`.** Create a Firebase project, add an Android app with your
   `applicationId`, and download **`google-services.json` into `Android/app/`**. It carries your
   project's keys, so it can't be bundled — and the Android build **fails without it**. (`plugin add`
   already applied the `com.google.gms.google-services` Gradle plugin + the `firebase-messaging` dep.)
2. **iOS — Push Notifications capability.** Enable **Push Notifications** on your App ID in the Apple
   Developer portal and create an **APNs auth key (.p8)** for your backend. `plugin add` already added
   the `aps-environment` entitlement as **`development`** (APNs sandbox / TestFlight) — switch it to
   **`production`** for App Store builds. A sandbox-vs-production mismatch is the #1 reason push
   silently never arrives.

## Validate without a backend (hand-send a test push)

Register on a real device, read the token off the screen, then:

**iOS (APNs sandbox, JWT from your .p8):**
```bash
curl -v --http2 \
  -H "apns-topic: <your-bundle-id>" -H "apns-push-type: alert" \
  -H "authorization: bearer <JWT-signed-with-.p8>" \
  -d '{"aps":{"alert":{"title":"Test","body":"hi"},"sound":"default"},"type":"new_booking"}' \
  https://api.sandbox.push.apple.com/3/device/<HEX_DEVICE_TOKEN>
```
Check that the two kinds stay apart:

1. App **open**, send → a banner + one event with `"mobiler_push":"received"`. No navigation.
2. **Background** the app, send, then open it from the **home screen** (not the banner) → **no** event.
3. Send again and **tap** the banner → one event with `"mobiler_push":"opened"`.
4. Force-quit, send, **tap** → `opened` arrives right after the relaunch (the launch-from-tap buffer).

**Android (FCM HTTP v1, OAuth bearer from a service account):**
```bash
curl -X POST -H "Authorization: Bearer <oauth-token>" -H "Content-Type: application/json" \
  https://fcm.googleapis.com/v1/projects/<project-id>/messages:send \
  -d '{"message":{"token":"<FCM_TOKEN>","data":{"type":"new_booking","title":"Test","body":"hi"}}}'
```
→ a tray notification. Run the same four checks as iOS: app open → `received`; backgrounded then
opened from the **launcher** → no event; **tap** → `opened` with that notification's payload; kill the
process (`adb shell am kill <pkg>` while backgrounded), send, tap → `opened` after the cold start. Send
two while killed and tap the **older** one: its payload arrives, not the newer one's.

> **`am force-stop` is not "killed".** Android delivers **nothing** (no FCM message, no broadcast) to a
> force-stopped app until the user starts it again. That's an Android rule, not a plugin bug. Use
> `am kill` on a backgrounded app, or swipe the app away from Recents, to test the dead-process path.

## Notes

- **Android tray icon + channel name.** By default the status-bar icon is a system info glyph and the
  channel (shown in Settings → Notifications) is named "Notifications". To brand them, add
  `Android/app/src/main/res/drawable/mobiler_push_icon.xml` (a white-on-transparent vector) and a
  string resource `<string name="mobiler_push_channel_name">Bookings</string>`. Both are picked up by
  name with no code change. (For FCM-displayed *notification* messages, set Firebase's
  `com.google.firebase.messaging.default_notification_icon` meta-data instead.)

- Requires `POST_NOTIFICATIONS` (Android 13+, added by the plugin) and notification authorization
  (requested by `register`).
- Tenant scoping is app-side: POST `{token, tenant, platform}` to your backend via `cx.post`/`cx.request` and let
  the backend target the right device set.
