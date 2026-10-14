# securestore — encrypted key/value storage (free, bundled)

```bash
mobiler plugin add securestore
```

For **secrets** (auth tokens, API keys) — not bulk data (use `cx.save` for general state).

```rust
cx.plugin("securestore", "set",    r#"{"key":"token","value":"abc"}"#, Msg::Stored),
cx.plugin("securestore", "get",    r#"{"key":"token"}"#,               Msg::GotToken),
cx.plugin("securestore", "delete", r#"{"key":"token"}"#,               Msg::Cleared),
```

- `input` is JSON. `set` → `ok:true`; `get` → `ok:true, output = value` (`""` if absent);
  `delete` → `ok:true` (also for a key that was never stored).
- **`ok:true` means stored.** A `set` or `delete` answers only once the change is on disk (Android
  `commit()`, the Keychain call on iOS). A write that fails answers `ok:false` with the reason in
  `output`, so you can wait for a `set` before acting on it (store a rotated token, then replay).
  On Android a failed write can still be held in memory until the app restarts, so treat `ok:false`
  as "not stored", not as "unchanged".
- **Order:** on Android, calls run one at a time in the order the core sent them. On iOS they can
  run at the same time, so wait for one call's answer before sending the next for the same key.
- **Android:** `EncryptedSharedPreferences` (AES-256, keys in the Android Keystore). Adds the
  `androidx.security:security-crypto` Gradle dep.
- **iOS:** the **Keychain** (`Security` framework — no package), items `…ThisDeviceOnly`.
- **Web:** graceful `ok:false` (browsers have no equivalent secure store; don't put real secrets in
  a web build).

**Pair with `biometric`** — gate a `get` behind an `authenticate` in your Rust core (see the
biometric plugin's `app-core-usage.rs`). Keychain/Keystore work on the simulator/emulator, so this
one is testable without a device (unlike biometric).
