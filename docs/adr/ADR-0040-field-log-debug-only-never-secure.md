# ADR-0040: The Android field debug log runs only in a debuggable build, logs lengths and hashes unless a second tag asks for raw text, and never logs a SECURE field's text

Status:        Accepted
Date decided:  2026-10-01
Deciding PRs:  #262
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt (`FieldLog`, `FieldSync`), demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt
Conformance:   xtask/tests/adr_conformance.rs::adr_0040_field_log_is_debug_only_and_never_logs_secure_text

## 1. Context (The Problem)

CLI 0.60.1 (#260) added a `MobilerField` debug log to the Android shell's `FieldSync`, so a text
field that shows the wrong text can be traced to what it was given. Its only gate was
`Log.isLoggable("MobilerField", DEBUG)`, a system property. Every message carried the raw text of
the edit, the app's value, the field and the pending queue, for every `TextField` and
`SearchField`, including `FieldKind::Secure`: `PasswordVisualTransformation` masks only what is
drawn.

The appointments team's security review of their 0.60.1 upgrade found the consequences (Moj Termin
request, 2026-10-01, quoted here): "There is no `FLAG_DEBUGGABLE` or `BuildConfig.DEBUG` check, and
generated apps ship with `isMinifyEnabled = false`. The code is therefore live in every release
APK." The property can be set persistently (`persist.log.tag.MobilerField`), and logcat reaches bug
reports, anyone with adb on an unlocked phone, and apps holding `READ_LOGS`. "Someone turns the log
on to chase a field bug on a staff phone and forgets to turn it off. From then on every sign-in
writes the password and the two-factor code into logcat." They held the 0.60.1 upgrade until this
is fixed.

## 2. Hypothesis

If the log is on only when the app is debuggable **and** the tag is set, logs each value as its
length and a 16-bit hash unless a second tag (`MobilerFieldValues`) is also set, and prints a field
that has ever been SECURE as `<secure len=N>` in every mode, then:

- no release build writes any field text to logcat, whatever properties the device carries;
- a debug build with the tag on never shows a password or a code, only lengths;
- the log still diagnoses the doubled-typing case: lengths and hashes show which value was adopted
  over which.

### 2.1. Refutation Conditions

- **Condition 1 — release builds are silent.** The gate requires `FLAG_DEBUGGABLE`, and the only
  write to the tag sits behind it.
- **Condition 2 — SECURE text never reaches the log.** `FieldLog.show` answers a SECURE value with
  its length before any branch that prints text; every log message interpolates a field's text only
  through `show`; a `TextField` notes its kind on its `FieldSync` before it is drawn.
  - **Validation Metric:** both, `adr_0040_field_log_is_debug_only_and_never_logs_secure_text` in
    `xtask/tests/adr_conformance.rs`, over the template's shell and every demo shell that has the log.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep the tag as the only gate** `[recorded: #260]`
  The 0.60.1 shape. Rejected: a system property is not a build boundary, and it is live in every
  release APK.
- **Option B — remove the log** `[reconstructed]`
  Rejected: the appointments team asked for the log to chase a doubled-typing bug in search
  fields, and a debug-only, redacted log still does that.
- **Option C — gate on `BuildConfig.DEBUG`** `[reconstructed]`
  Rejected: AGP 8 generates `BuildConfig` only with `buildFeatures.buildConfig = true`, a template
  change every existing app would merge through `mobiler upgrade`. `ApplicationInfo.FLAG_DEBUGGABLE`
  is the same fact, read at runtime with no build change.
- **Option D — debuggable gate, length-and-hash by default, raw text behind a second tag, SECURE
  never** `[recorded: Moj Termin request, 2026-10-01, quoted here]`
  The team's request: "Gate on the app being debuggable as well as the tag", "Never the value of a
  SECURE field, even in debug: log `<secure len=N>`", and "Log lengths, cursor and composition ranges
  and a short hash, and require a second, explicit tag (e.g. `MobilerFieldValues`) before any raw text
  is logged." Chosen.

## 4. Decision & Rationale for Corroboration

Option D. A `FieldLog` object holds the gate, set once in `onCreate` from `applicationInfo.flags`
and the two tags. `FieldLog.show` is the only way a field's text enters a message. SECURE is
sticky per field (`FieldSync.noteKind`): a show-password toggle that flips the kind to plain text
keeps the field logging as SECURE, since its text is still the password. A SECURE value logs no
hash either: a 16-bit hash of a six-digit code or a short PIN is trivially reversed.

The iOS and web shells have no field log. If either gains one, this decision binds it.

**Mutation proof:**
- Replacing `FieldLog.show(next.text, secure)` with `'${next.text}'` in the template's `onEdit`
  message failed the test: "a field log message interpolates text without FieldLog.show".
- Dropping `debuggable && ` from the template's gate failed it: "the MobilerField tag is used
  outside the gate and the gated write".
- Moving the `values ->` branch above `secure ->` in the template's `show` failed it: "show must
  check secure before printing raw values".
- Removing `sync.noteKind(widget.kind)` from the template's TextField failed it: "the TextField must
  note its kind on its FieldSync".
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a release build writes no field text to logcat, and a debug build shows no secret
  even with both tags on.
- **Negative:** the default log shows hashes, not text, so a developer must set the second tag to
  read values. That is deliberate.
- **Negative:** a 16-bit hash of a short non-secure value (a phone number, a search term) can be
  brute-forced, and a one-character value hashes to its own character code (`h` logs as `#0068`). It is debug-only and opt-in; raw text is one tag away anyway.
- **Negative:** only `FieldKind::Secure` is treated as secret. Personal data in plain fields (names,
  notes) is redacted by default, but logged as text once `MobilerFieldValues` is on.
- **Negative:** the conformance test checks the source's shape, not runtime output. A new log call
  written some other way (not `FieldLog.d`) would escape it; review must catch that.
- **Negative:** apps on 0.60.1 keep the old log until `mobiler upgrade` merges the new shell.
