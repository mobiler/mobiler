# ADR-0037: The scaffold's Android backup rules exclude `securestore`'s file from cloud backup and device transfer; everything else, including `cx.save`'s plaintext state, stays backed up

Status:        Accepted
Date decided:  2026-09-30
Deciding PRs:  #256
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/main/res/xml/{backup_rules,data_extraction_rules}.xml, mobiler/templates/Android/app/src/main/AndroidManifest.xml (allowBackup + the two rule attributes), mobiler/plugins/securestore/android/SecureStorePlugin.kt (recovery), mobiler-core/src/lib.rs (Cx::save doc)
Conformance:   xtask/tests/adr_conformance.rs::adr_0037_template_backups_exclude_the_secure_store

## 1. Context (The Problem)

The scaffold's `AndroidManifest.xml` sets `android:allowBackup="true"` and points at
`backup_rules.xml` / `data_extraction_rules.xml`. Until this record, both rule files were Android
Studio's empty samples, so Auto Backup copied every shared-preferences file to the user's cloud
backup and to a new phone.

Two of those files are the framework's:

- `mobiler.xml`: `cx.save`'s state (ADR-0031), stored as plaintext.
- `mobiler_secure.xml`: the `securestore` plugin's values, encrypted with a key held in *this*
  device's Android Keystore. A backup never carries Keystore keys, so a restored copy can't be
  decrypted. `EncryptedSharedPreferences.create` then threw on every call to the plugin.

The appointments team raised both in its field notes (their `docs/mobiler-feedback.md`, §3):
plaintext state in cloud backups, and a manifest that opts into backup without saying so.

## 2. Hypothesis

If the scaffold's rules exclude `mobiler_secure.xml` from cloud backup and device transfer, keep
everything else, and `securestore` starts an empty store when its file can't be decrypted, then:

- a restored or transferred app never carries secrets it can't read, and `securestore` never
  crashes on one;
- an app's ordinary state (`cx.save`, SQLite databases, files) still survives a new phone;
- the choice is visible in two short files an app can change.

### 2.1. Refutation Conditions

- **Condition 1 — the exclusion is in every rule set.** `backup_rules.xml` (Android 11 and older)
  and both `<cloud-backup>` and `<device-transfer>` in `data_extraction_rules.xml` exclude
  `sharedpref/mobiler_secure.xml`, as live XML, and the manifest points at both files.
  - **Validation Metric:** `adr_0037_template_backups_exclude_the_secure_store` in
    `xtask/tests/adr_conformance.rs`.
- **Condition 2 — an unreadable store recovers.** `SecureStorePlugin` deletes a file it can't open
  and opens a fresh one. Review; no Android unit tests run in CI.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep the empty sample rules (back up everything)** `[recorded: appointments docs/mobiler-feedback.md §3]`
  The state before this record. A restored secure store is unreadable and crashed the plugin.
- **Option B — `allowBackup="false"`** `[reconstructed]`
  Safe for secrets, but every app would lose its data on a new phone, including the local-first
  apps this framework is used for (Saldo keeps its ledger in SQLite).
- **Option C — also exclude `mobiler.xml` (`cx.save`)** `[reconstructed]`
  More private by default, but `cx.save` is the app's restorable state by design (ADR-0031). Its
  doc comment now says it is plaintext and backed up; an app that keeps anything sensitive there is
  told to use `securestore`, or can add one line to its rules.
- **Option D — exclude only the secure store** `[reconstructed]`
  Chosen: the one file that cannot survive a restore.

## 4. Decision & Rationale for Corroboration

Option D. Both rule files carry `<exclude domain="sharedpref" path="mobiler_secure.xml"/>`, with a
comment saying why and that `cx.save`'s state is plaintext and included. `SecureStorePlugin` opens
its store through `open()`, and on failure deletes `mobiler_secure` and opens it again, so the app
sees an empty store (`get` answers `""`) instead of an exception. `Cx::save`'s doc comment names
the plaintext storage and Auto Backup. The demos carry the same rule files.

**Mutation proof:**
- Commenting out the exclude in `backup_rules.xml` failed the test: "ADR-0037: backup_rules.xml
  must exclude mobiler_secure.xml".
- Removing the exclude from `<device-transfer>` failed it: "ADR-0037: <device-transfer> must
  exclude mobiler_secure.xml".
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a new phone restores the app's data but not secrets it could never decrypt, and
  `securestore` no longer crashes after a restore.
- **Negative:** secrets don't move to a new phone. The app must sign the user in again (or
  re-enrol biometrics) after a restore or transfer.
- **Negative:** `cx.save`'s state is still plaintext in the user's backup. That is a documented
  default, not a guarantee; an app that ignores the doc comment leaks what it saves.
- **Negative:** the recovery wipes the secure store whenever it can't be opened, including after
  a Keystore reset on the same device. The values were unreadable either way, but the app only
  notices by finding them empty.
- **Negative:** the rules are template files. An existing app gets them through
  `mobiler upgrade`; one that edited its own rule files gets a `.mobiler-new` to merge by hand.
