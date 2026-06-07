# push-firebase-only — remote push, Firebase everywhere (free, bundled)

```bash
mobiler plugin add push-firebase-only
```

> ⚠️ **Experimental — not yet device-tested**, and **mutually exclusive with the `push` plugin** (install
> one or the other; both register `case "push"`, which collides at build). Mobiler itself is experimental.

The **Firebase-only** alternative to [`push`](../push/README.md). Where `push` uses **native APNs on
iOS + FCM on Android** (your backend speaks both), `push-firebase-only` uses the **Firebase iOS SDK on
iOS too** — so `register` returns an **FCM token on both platforms** and your backend speaks **one FCM
API with one token type**.

It registers under the **same cx name `"push"`**, so your app code is identical — you just install a
different plugin:

```rust
cx.plugin("push", "register", "", Msg::PushToken),   // → {"token":"…","platform":"fcm"} on BOTH platforms
cx.subscribe("push", "push", "events", "", Msg::PushEvent),
```

## How it differs from `push`

- **iOS** = Firebase iOS SDK (via Swift Package Manager) instead of raw APNs. The shell's AppDelegate
  forwards the raw APNs token to `Messaging.apnsToken` (FCM relays to iOS through APNs under the hood),
  and `register` returns the **FCM registration token**. Inbound notifications still arrive via APNs →
  the AppDelegate/`PushBridge`, so the events stream is **unchanged** from `push`.
- **Android** = identical to `push` (FCM).
- **Backend** = one FCM HTTP v1 endpoint, one token type. (With plain `push` you'd send APNs for iOS +
  FCM for Android.)

**Pick `push`** if you want iOS to stay free of the Firebase SDK (native APNs). **Pick
`push-firebase-only`** if you'd rather have one unified FCM backend.

## Setup (the manual steps `plugin add` can't do)

1. **iOS — `GoogleService-Info.plist`.** Add an iOS app in your Firebase project and drop its
   `GoogleService-Info.plist` into `iOS/Sources/` (Firebase reads it at `FirebaseApp.configure()`). It
   carries your project keys, so it can't be bundled.
2. **iOS — relay key.** Enable the **Push Notifications** capability on your App ID, and **upload your
   APNs auth key (`.p8`) to the Firebase console** (Project Settings → Cloud Messaging) so FCM can
   deliver to iOS via APNs. The `aps-environment` entitlement was added as `development` (sandbox /
   TestFlight); switch it to `production` for App Store builds.
3. **Android — `google-services.json`.** Add an Android app in the same Firebase project and drop
   `google-services.json` into `Android/app/` — the build fails without it.

## Notes

- The Firebase iOS SDK is a **large SPM dependency** and lengthens iOS builds — that's the deliberate
  cost of unifying on FCM, and why this is opt-in rather than the default `push`.
- Validate the signed receipt / token server-side, and POST `{token, tenant, platform}` to your backend
  (same stance as `push`).
