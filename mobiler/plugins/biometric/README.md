# biometric — Face ID / fingerprint authentication (free, bundled)

```bash
mobiler plugin add biometric
```

```rust
cx.plugin("biometric", "authenticate", "Unlock your account", Msg::Authed),
Msg::Authed(resp) => { if resp.ok { /* unlocked */ } else { /* resp.output = reason */ } }
```

- `op` = `authenticate`; `input` = the prompt title/reason (optional). `ok:true` on success;
  `ok:false` with a reason on cancel / failure / no enrolled biometric / no hardware. Falls back to
  the device passcode/credential.
- **Android:** `androidx.biometric` `BiometricPrompt` — **requires a `FragmentActivity` host**. The
  Mobiler shell's `MainActivity` extends `FragmentActivity` (changed framework-wide for this plugin),
  so a fresh `mobiler new` app works out of the box. Adds the `androidx.biometric` Gradle dep.
- **iOS:** `LocalAuthentication` (`LAContext`, system framework). Needs **`NSFaceIDUsageDescription`**
  (added by the manifest) for Face ID.
- **Web:** graceful `ok:false`.

**Pair with `securestore`** to protect secrets: authenticate, then read the token (composed in your
Rust core — see `app-core-usage.rs`). Test on **real hardware** (no biometric sensor on the
emulator/simulator; iOS via TestFlight, Android on a device with a fingerprint/face enrolled).
