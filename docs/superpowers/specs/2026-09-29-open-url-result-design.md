# `open_url` with a result

**Request:** the follow-up to `docs/open-url-no-handler.md` (fixed for Android in CLI 0.57.1): the
result "should also reach `then` as `ok: false`, so the app can show 'this device cannot place
calls'". Part of the Moj Termin design release. Approved 2026-09-29.

## Decisions

1. **`cx.open_url` is unchanged** (a byte-compatible notification). New:
   `cx.open_url_then(url, then)` sends the same `browser`/`open` call as a request.
   - `ok: true` means the system handed the link to an app.
   - `ok: false` carries a reason: `"no app can open this link"`, `"invalid url"` or `"blocked"`.
2. **Honest results.**
   - Android: already honest (0.57.1).
   - iOS: `await UIApplication.shared.open(url)` returns whether an app took it.
   - Web: `window.open` returning null (a blocked pop-up) is `ok: false, "blocked"`. A browser can't
     know whether the desktop can place a `tel:` call, which is documented.
3. **Scope:** core + web + barbershop iOS. The other demos' iOS shells keep the always-ok plugin;
   the templates take barbershop's at release.

## Demo

The pinned bar's "Call" is `Msg::DialShop`: `open_url_then("tel:…")`. On `ok: false` it shows a
snackbar, "This device can't place calls".

## Verification

- **Core:** `open_url` is still a notification; `open_url_then` is a `browser`/`open` request whose
  `then` gets the response.
- **Barbershop:** `DialShop` sends the request; `Dialed(false)` shows the snackbar and
  `Dialed(true)` shows nothing.
- **Web CDP:** a stubbed `window.open` that returns a window gives no snackbar; one that returns
  null shows "This device can't place calls".
- **Android AVD:** this image has a dialer, so "Call" opens it (`ok: true`). The no-handler path was
  AVD-verified with 0.57.1.
- **iOS:** CI compile only.

## After review (2026-09-29)

- iOS: a declined confirmation (the `tel:` "Call …?" prompt) also reports `false`, so on `false`
  the shell asks `canOpenURL`. Can open → `"cancelled"`; can't → `"no app can open this link"`.
  `tel` and `sms` are declared in `LSApplicationQueriesSchemes` (template at release).
- The demo branches on the reason: `"cancelled"` → nothing; `"blocked"` → "Allow pop-ups to place
  calls"; anything else → "This device can't place calls".
- Web: the request branch must stay await-free (`window.open` in the click's task).
- **Needs a device check before release:** iPhone Cancel on the Call prompt → no snackbar; iPad
  without calling → the snackbar.
