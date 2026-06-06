# push — remote push notifications, APNs/FCM (free, bundled)

```bash
mobiler plugin add push
```

Server-sent push for the native shells — the companion to the local `notifications` plugin. Two
surfaces, both riding existing Mobiler primitives (no ABI change):

```rust
// 1) Get the device token (one-shot) → POST it (with your tenant) to your backend.
cx.plugin("push", "register", "", Msg::PushToken),
Msg::PushToken(r) => if r.ok { /* r.output = {"token":"…","platform":"apns"|"fcm"} */ },

// 2) Subscribe to inbound pushes (the streaming primitive) — do this at startup so a tap that
//    launched the app (buffered by the shell) isn't missed.
cx.subscribe("push", "push", "events", "", Msg::PushEvent),
Msg::PushEvent(r) => if r.ok {
    // r.output = the notification's JSON payload (foreground-received OR tapped),
    //            or {"type":"token_refresh","token":"…"} when the OS rotates the token.
},
```

- **`register`** asks for notification authorization, registers with APNs/FCM, and returns the device
  token. **`events`** (subscribe) delivers each received/tapped notification and token rotations;
  `cx.unsubscribe("push")` stops it.
- **iOS** = native **APNs** via system frameworks (no third-party SDK). The shell's `AppDelegate`
  receives the token + notification callbacks and forwards them to `PushBridge`; this plugin adapts
  them to the cx ABI. The shell buffers a launch-from-tap payload until the core subscribes.
- **Android** = **Firebase Cloud Messaging** (the only way to get a device token on stock Android).
  A `FirebaseMessagingService` feeds `onMessageReceived` / `onNewToken` into an in-process bus that
  buffers until the core subscribes. Send **data** messages from your backend so the app controls
  display + the event stream fires consistently (foreground and background).
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
Backgrounded → a banner; foreground → the `events` stream fires live. Force-quit, send, tap → the
payload still reaches the app after relaunch (the launch-from-dead buffer).

**Android (FCM HTTP v1, OAuth bearer from a service account):**
```bash
curl -X POST -H "Authorization: Bearer <oauth-token>" -H "Content-Type: application/json" \
  https://fcm.googleapis.com/v1/projects/<project-id>/messages:send \
  -d '{"message":{"token":"<FCM_TOKEN>","data":{"type":"new_booking","title":"Test","body":"hi"}}}'
```
→ a tray notification + the `events` stream fires.

## Notes

- Requires `POST_NOTIFICATIONS` (Android 13+, added by the plugin) and notification authorization
  (requested by `register`).
- Tenant scoping is app-side: POST `{token, tenant, platform}` to your backend via `cx.http` and let
  the backend target the right device set.
