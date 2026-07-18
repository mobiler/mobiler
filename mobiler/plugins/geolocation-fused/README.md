# geolocation-fused — device location via Play Services FusedLocationProvider (free, bundled)

```bash
mobiler plugin add geolocation-fused
```

A higher-accuracy / lower-power alternative to the default **`geolocation`** plugin. On Android it uses
Google **Play Services FusedLocationProvider** (fuses GPS + Wi-Fi + cell + sensors) instead of the
framework `LocationManager`. It registers under the **same cx name `"geolocation"`**, so your app code is
identical — install **`geolocation` OR `geolocation-fused`**, not both.

```rust
cx.plugin("geolocation", "get", "", Msg::GotLocation),
Msg::GotLocation(r) => if r.ok { /* r.as_text() = "lat,lng" */ },
```

- **Android** — `FusedLocationProviderClient.getCurrentLocation(PRIORITY_HIGH_ACCURACY)` (falls back to
  the last cached location). Needs **Google Play Services** on the device + `ACCESS_FINE_LOCATION` /
  `ACCESS_COARSE_LOCATION` (added by `plugin add`); the first call fires the permission prompt and returns
  `"permission requested — try again"`.
- **iOS** — identical to `geolocation` (CoreLocation has no "fused" concept): a one-shot
  `requestLocation` after when-in-use authorization. Adds `NSLocationWhenInUseUsageDescription`.
- **web** — native-only; returns `ok:false`.

**When to use which:** `geolocation-fused` for best accuracy (the Play Services dep is fine for most
apps); the default **`geolocation`** for a dependency-free build or devices without Play Services. They
are mutually exclusive (both register `"geolocation"`).
