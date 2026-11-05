# Share an image file, and save an image to the photo library

Date: 2026-11-05
Requested by: the appointments app (Moj Termin admin), issue #711, 2026-11-04
Constrained by: ADR-0002, ADR-0003, ADR-0007, ADR-0009, ADR-0010, ADR-0011, ADR-0013, ADR-0015, ADR-0021, ADR-0023, ADR-0027, ADR-0035, ADR-0038, ADR-0039, ADR-0049, ADR-0063

## 1. Intent

A salon owner opens their booking page's QR code in the Moj Termin admin app. They want to:

1. **share** it as an image (Viber, WhatsApp, an Instagram story), and
2. **save** it to the phone's Photos/Gallery to print or post later.

The QR is a PNG at a public HTTPS URL (`GET /api/v1/public/orgs/{slug}/page/qr`, 1024 px). Today the
app opens it in the browser with `cx.open_url` and tells the owner to save it from there, which takes
them out of the app.

The appointments team proposed two new core builders (`cx.share_file`, `cx.save_image`). **The
maintainer chose plugins instead** (2026-11-05): every platform capability is an opaque plugin
effect (ADR-0002), and saving to Photos needs an iOS usage string, which only an app that uses it may
carry (ADR-0035). So:

- the existing **`sharefile`** plugin is extended and its Android bug fixed;
- a new bundled plugin, **`saveimage`**, saves an image to the photo library;
- the **web shell** (`mobiler-web`) answers both plugins with what a browser can do.

No core change, no ABI change, no new `Cx` builder. Apps call
`cx.plugin("sharefile", "file", input, then)` and `cx.plugin("saveimage", "save", input, then)`.

## 2. Goals, non-goals, anti-goals

**Goals**
- Share an image with an optional text from an https URL, a sandbox path, `file://` or `content://`,
  without the app downloading it first. Viber and WhatsApp receive an *image*, not a link.
- Save an image to Photos/Gallery on Android 8+ (API 26) and iOS 17+, with an honest `ok`.
- A denied iOS permission gives `ok: false` with a reason. Nothing crashes.
- Existing `sharefile` callers (a bare URI as input) keep working unchanged.
- The web answers both instead of "plugin not available".

**Non-goals**
- Saving videos, or saving into a named album. (`saveimage` is images only. An album option can come
  later as an additive input field.)
- Reading from the photo library. That is `cx.pick_photo`.
- Sharing several files at once.
- Requests that need a login *cookie* on the web (the web fetch uses the browser's default `same-origin` credentials). Request headers are supported (§3.1).
- A core helper/typed builder. Apps that want one can wrap `cx.plugin` themselves.

**Anti-goals** (failure even if it "works")
- The share target gets a URL or a text-only message instead of the PNG.
- Any scaffold or app that does not install `saveimage` gains a photo-library permission or usage
  string (ADR-0035).
- `ok: true` when the image was not saved (ADR-0013).
- An app crash on a missing permission, a failed fetch, or a non-image source.

## 3. The `sharefile` plugin (extended)

### 3.1 Input

Op stays `file`. The input is either:

- **a bare handle string** (today's form, unchanged): `content://…`, `file://…`, or an absolute path; or
- **a JSON object**, recognised when the trimmed input starts with `{`:

```json
{ "source": "https://example.com/qr.png", "mime": "image/png", "text": "Book with us", "name": "qr.png" }
```

| field | required | meaning |
|---|---|---|
| `source` | yes | `https://…`, `content://…`, `file://…`, an absolute path, or a sandbox-relative path (the `files` plugin's paths: Android `filesDir`, iOS Documents) |
| `mime` | no | MIME type. Default: from the fetch's `Content-Type`, else from the file extension, else `application/octet-stream` |
| `text` | no | text sent with the file (Android `EXTRA_TEXT`; iOS a second activity item; web `ShareData.text`) |
| `name` | no | file name the receiver sees. Default: the source's last path segment; an extension is added from `mime` when missing |
| `headers` | no | object of request headers for an https fetch (e.g. `Authorization`), as `files`/`transfer` downloads take. Ignored for local sources |

`http://` sources are refused (`insecure_source`), consistent with ADR-0038 (cleartext is debug-only)
and iOS ATS. Other schemes are refused with `unsupported_source`.

### 3.2 Behaviour

- **https source:** the shell fetches it natively (OkHttp on Android, `URLSession` on iOS) into a
  share cache dir (Android `cacheDir/share/`, iOS `Caches/share/`), named `name`. Bytes never enter
  the core (ADR-0027). A non-2xx or network failure is `fetch_failed`. The share dir is emptied of
  files older than 24 hours at the start of each share call (the receiving app may still be reading a
  recent one). A size cap of 50 MB applies (`too_large`).
- **Android:** `content://` is used as is. Any file path (including a fetched one) is **copied into
  `cacheDir/share/`** and wrapped with the app's `FileProvider`. The template's `file_paths.xml`
  gains `<cache-path name="share" path="share/" />`. **This fixes today's bug:** a `files`-plugin
  download lives in `filesDir`, which the FileProvider does not cover, so sharing it threw "Failed to
  find configured root". The intent sets `type` to the MIME type, `EXTRA_STREAM`, `ClipData` (so the
  read grant reaches the target on API 29+ and the chooser shows a preview), and `EXTRA_TEXT` when
  given. The plugin checks at runtime that the provider covers `share/`. If it doesn't (an app whose
  template predates this), it answers `ok: false, "provider_missing: run mobiler upgrade"` instead of
  throwing.
- **iOS:** the file (fetched or local) is shared as a file URL with the right extension. When the
  MIME type is `image/*` and the bytes decode as a `UIImage`, the activity item is an item provider
  that offers both the image and the file URL, so Photos-style targets and messengers see an image.
  `text` is a second activity item.
- **Reply** (ADR-0013, unchanged meaning): Android `ok: true, "shared"` once the chooser is presented
  (Android cannot observe the choice). iOS `ok: true, "shared"` / `ok: false, "cancelled"` from the
  completion handler. Errors are `ok: false` with a reason code (§5).

## 4. The new `saveimage` plugin

### 4.1 Input

Op `save`. Input is JSON:

```json
{ "source": "https://example.com/qr.png", "name": "moj-termin-qr.png" }
```

`source` takes the same kinds as `sharefile` (§3.1). `name` is optional: the display name in the
gallery (Android `DISPLAY_NAME`; iOS `originalFilename`), with the same defaulting rule as above.

### 4.2 Behaviour

- Fetch an https source as in §3.2 (same refusals and cap). The file must decode as an image
  (`BitmapFactory.decodeFile` bounds-only on Android, `UIImage`/`CGImageSource` on iOS); otherwise
  `not_an_image`. The bytes are saved **as they are** (a PNG stays a PNG). They are not re-encoded.
- **Android API 29+:** `MediaStore.Images` insert with `RELATIVE_PATH = Pictures/<app label>`,
  `IS_PENDING = 1`, write, then `IS_PENDING = 0`. No permission is needed.
- **Android API 26–28:** `WRITE_EXTERNAL_STORAGE` with `maxSdkVersion=28`, declared in the plugin's
  `permissions` (so only apps that install it get it). When not yet granted, the plugin **waits for
  the user's answer** through a small transparent helper Activity (`SaveImagePermissionActivity`,
  declared via `manifest_application`, the same shape as `files`' `FileExportActivity`) that uses
  `registerForActivityResult(RequestPermission)`. Granted → the save continues in the same call, so
  the first tap reports `ok: true`. Refused (or "don't ask again") → `denied`. This is deliberately
  not the older fire-and-forget pattern (`"permission requested — try again"`) other plugins use,
  because that can't report a refusal (ADR-0013). The write goes to
  `Pictures/<app label>/` and is scanned into MediaStore.
- **iOS:** `PHPhotoLibrary.requestAuthorization(for: .addOnly)`, then
  `PHAssetCreationRequest.forAsset().addResource(with: .photo, fileURL:, options:)` with
  `originalFilename = name`. `.denied`/`.restricted` → `denied`. The manifest's `[ios.info_plist]`
  adds `NSPhotoLibraryAddUsageDescription = "Save images to your photo library."` (an app edits the
  wording in its own Info.plist, as with every plugin).
- **Reply:** `ok: true, "saved"` only once the platform confirms the write (MediaStore update
  returned, `performChanges` succeeded). Otherwise `ok: false` with a reason code (§5).

## 5. Reply codes (both plugins)

`ok: false` output is `<code>` or `<code>: <detail for logs>`. An app matches the part before the
first `:`. The codes are:

`bad_input` (missing/unparseable `source`), `insecure_source`, `unsupported_source`, `fetch_failed`,
`too_large`, `not_found` (local file missing), `not_an_image` (saveimage), `denied` (saveimage
permission), `cancelled` (iOS/web share dismissed), `no_activity` (Android, nothing in the
foreground), `provider_missing` (Android, old template), `unavailable` (web: the browser can't),
`failed` (anything else, with detail).

Existing bare-handle `sharefile` callers keep today's outputs (`"shared"`, `"cancelled"`). Today's
free-text error strings become these codes, and no app matches on those today.

Because apps are told to match on these codes, they are a compatibility promise: the **CLI PR
records them in an ADR** (the codes are stable; new codes may be added; an app treats an unknown code
as `failed`). Like ADR-0013 the property lives in native plugin code with no unit-test harness, so
its conformance is review plus the device checks. The web shell's mapping is a pure Rust function
with a unit test, and that test is proven by a deliberate mutation (CLAUDE.md).

## 6. Web shell (`mobiler-web`)

The web shell answers both plugins built-in, the way it already answers the installable `transfer`
plugin (ADR-0027). This is recorded as a new ADR in the libs PR: *the web shell answers an installable
plugin's op built-in when a browser can do the job, and answers `ok: false, "plugin '…' not
available"` otherwise; nothing is installed on the web.*

- **`sharefile/file`:** resolve `source` to a `Blob`: `fetch` (with `headers`) for https, or a
  `blob:` handle (the web photo pipeline's handles, ADR-0049). If
  `navigator.canShare({files:[file]})` is true, call `navigator.share({files, text})`: resolve →
  `ok: true, "shared"`, `AbortError` → `cancelled`. Otherwise (file sharing unsupported, or
  `NotAllowedError` because the tap's user activation expired during the fetch, common on Safari),
  fall back to a download (an object URL on an `<a download=name>` click, which pop-up blockers
  don't stop) → `ok: true, "downloaded"`.
- **`saveimage/save`:** a browser has no photo library. It fetches and downloads as above →
  `ok: true, "downloaded"`.
- **No `window.open` fallback.** Every web plugin call runs after an `await`, and a `window.open`
  there is blocked as unrequested (the rule noted at `mobiler-web/src/lib.rs` `browser/open`). A
  fetch that fails, including a cross-origin URL without CORS headers, is `ok: false,
  "fetch_failed"`. The docs say so: **on the web a cross-origin `source` needs CORS**, and an app
  whose API is elsewhere can still fall back to `cx.open_url` itself.

The output word (`shared` / `downloaded`) tells the app what actually happened, so it can word its
toast.

## 7. Release shape (ADR-0009)

The `file_paths.xml` line is made and built in `demos/barbershop` first and then ported to the
template (ADR-0015). The CLI PR runs `mobiler upgrade --apply` in Saldo, coffee, todo and
fullstack-todo's mobile app so they carry it too (ADR-0063).


1. **Libs PR:** `mobiler-web` minor bump (0.49 → 0.50) with §6, the new ADR, and README updates. No
   core/ui change. Publish `mobiler-web` (maintainer confirms the publish).
2. **CLI PR:** `sharefile` changes, the new `saveimage` plugin, the template `file_paths.xml` line,
   the template's `mobiler-web` pin → 0.50, README/site/capabilities updates. CLI minor bump
   (0.73 → 0.74). Tag and publish (maintainer confirms).

Upgrading apps: `mobiler upgrade` brings the `file_paths.xml` line and reports `sharefile` as
drifted. Re-running `mobiler plugin add sharefile` refreshes it (ADR-0011).
`mobiler plugin add saveimage` installs the new one.

## 8. Verification

- Unit tests where there is Rust: the web shell's input parsing and code mapping (pure functions),
  and the CLI's plugin-manifest tests (the new plugin is listed, its permission and plist key are
  inserted only when added).
- **Android emulator, API 33 and API 26:** a demo screen (barbershop already uses `sharefile`) shares
  the https QR-style PNG and a `files`-downloaded file. The chooser shows an image preview. Save puts
  it in the gallery (checked with `adb shell content query` on MediaStore). On API 26 the first tap asks for the
  permission and, once granted, saves in the same call (`ok: true`); refusing gives `denied`.
- **iOS 17 simulator:** share sheet shows the image. Save with permission granted → Photos has it.
  Denied (`xcrun simctl privacy … revoke photos-add`) → `ok: false, denied`, no crash.
- **Web:** headless Chrome via CDP: fallback path (download) verified; `navigator.share` path checked
  by review (headless has no share sheet).
- **iPhone + real Viber/WhatsApp:** done by the maintainer/appointments team. Emulators can't prove
  "arrives as an image, not a link".

## 9. Left to the builder

- Exact Kotlin/Swift structure, naming inside the plugins, the iOS item-provider details.
- Where the demo button lives (barbershop's existing share card is the natural spot), and the sample
  image URL it uses.
- Whether the 24-hour share-dir prune shares a helper between the two plugins (each plugin is copied
  into apps separately, so duplication across plugin files is acceptable).

## 10. Calls made by the designer (not discussed with the maintainer)

- `http://` sources refused; 50 MB cap; 24-hour share-dir prune.
- Reply codes as in §5, with today's free-text `sharefile` errors replaced.
- `saveimage` saves bytes unchanged (no re-encode), images only, no album option.
- Plugin name `saveimage`, op `save`; `sharefile` keeps op `file` and gains a JSON input form.
- Web: share falls back to download (also on `NotAllowedError`); no `window.open` fallback; cross-origin needs CORS; save = download.
- Android 26–28: a helper Activity waits for the permission answer instead of the repo's fire-and-forget pattern.
- Optional `headers` on an https source; no cookie/credential mode on the web.
- The reply codes get their own ADR in the CLI PR; the web shell's routing of installable plugins gets one in the libs PR.
- Libs PR then CLI PR, minor bumps on both.
