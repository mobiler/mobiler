# ADR-0061: An app's data, its SQLite database included, stays in the platform's backup and device transfer on Android and iOS; a finance app such as Saldo is not an exception, and the only exclusion is the secure store (ADR-0037)

Status:        Accepted
Date decided:  2026-10-20
Deciding PRs:  #306
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/main/res/xml/{backup_rules,data_extraction_rules}.xml, demos/saldo/Android/app/src/main/res/xml/{backup_rules,data_extraction_rules}.xml, mobiler/plugins/sqlite/ios/SqlitePlugin.swift (the database in Documents, backed up by iCloud)
Conformance:   xtask/tests/adr_conformance.rs::adr_0061_app_data_stays_in_backups

## 1. Context (The Problem)

ADR-0037 made the Android template exclude only the secure store from cloud backup and device transfer. It kept
`cx.save`'s state, SQLite databases and files, and rejected turning backup off because local-first apps would lose
their data on a new phone. It decided this for Android only.

The security review of #305 raised it again for Saldo, the finance showcase. Saldo's SQLite ledger goes into Google
cloud backup and phone-to-phone transfer. A backup restored onto another device, the review said, has no biometric
lock in front of it. The maintainer first agreed to exclude the database (2026-10-20), then questioned it: "they can
not backup someone else database, so if they get it from their own account, should they just be able to use it?"

What was checked:
- **The backup is tied to the user's own account.** An Android backup restores only onto a phone signed in to the
  same Google account, and it is end-to-end encrypted when the phone has a screen lock. An iCloud backup restores
  only to the same Apple ID.
- **The lock comes back with the data.** Saldo keeps its Security setting in the same SQLite database
  (`save_setting(cx, "lock", …)`), so a restored Saldo asks for the new phone's fingerprint or face. The review's
  premise was wrong.
- **iOS was never decided.** The sqlite plugin keeps `mobiler.db` in Documents, which iCloud backs up.

## 2. Hypothesis

If an app's data stays in backup and transfer on both platforms, users keep their history across a new or reset
phone. That includes users who never export. The only exposure is to someone who already controls the user's Google
account or Apple ID, who could restore the backup onto their own device. Such a person has far more than the app's
data, so excluding the database buys little and costs every honest user their data.

### 2.1. Refutation Conditions

- **Condition 1: no app data is excluded on Android.** The template's and Saldo's `backup_rules.xml` and
  `data_extraction_rules.xml` exclude nothing but `mobiler_secure.xml`. Saldo's rule files sit under `res/`, which
  ADR-0060's test skips, so this test checks them.
  - **Validation Metric:** `adr_0061_app_data_stays_in_backups`.
- **Condition 2: no file is excluded from iCloud backup.** No template iOS source, bundled plugin or Saldo source
  sets `isExcludedFromBackup`.
  - **Validation Metric:** the same test.
- **Condition 3: a restored app is still locked.** An app that locks its data keeps the lock setting with that data,
  so a restore brings both.
  - **Validation Metric:** Saldo's `save_setting("lock")` in the SQLite settings table; review.

## 3. Considered Options & Rationale for Refutation

- **Option A: exclude the database for Saldo only** `[recorded: the #305 security review; the maintainer's first answer, 2026-10-20]`
  Rejected. A user who never exported would lose every entry on a new or reset phone, and the lock premise was wrong
  (Condition 3). It would also need a framework option (ADR-0060 forbids a hand edit in Saldo).
- **Option B: a framework option to exclude an app's database** `[recorded: memory note saldo-backup-exclusion-todo, 2026-10-20]`
  Not built. No app asks for it; an app that needs it can still edit its own two rule files, as ADR-0037 says.
- **Option C: turn backup off (`allowBackup="false"`, `isExcludedFromBackup` on Documents)** `[recorded: ADR-0037 Option B]`
  Rejected for the same reason ADR-0037 gave: local-first apps lose their data on a new phone.

## 4. Decision & Rationale for Corroboration

Option: keep app data in backups on both platforms, as ADR-0037 does for Android, now stated for iOS and for the
SQLite database, and with Saldo as the finance example that confirms it.

**Mutation proof:** the decision was broken three times on purpose. The test failed each time; each file was then
restored, and the test passed again.
- `<exclude domain="database" path="mobiler.db"/>` was added to Saldo's `<cloud-backup>`
  (`ADR-0061: demos/saldo/…/data_extraction_rules.xml excludes app data from backup: …`).
- `<exclude domain="sharedpref" path="mobiler.xml"/>` was added to the template's `backup_rules.xml`
  (`ADR-0061: mobiler/templates/…/backup_rules.xml excludes app data from backup: …`).
- The iOS sqlite plugin set `isExcludedFromBackup` (`ADR-0061: …/SqlitePlugin.swift excludes a file from iCloud backup`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a user's data survives a new phone, a reset or a restore, without an export.
- **Positive:** the question has a recorded answer, so a later review that raises it again has the reasons to hand.
- **Negative:** someone in control of a user's Google account or Apple ID can restore that user's app data onto their
  own device. An app's lock (Saldo's biometric setting) still applies, because the setting travels with the data.
- **Negative:** an app that does need an exclusion edits its own rule files. Saldo can't, because its shells must
  equal the template (ADR-0060); if it ever needs one, that becomes a framework option and a new record.
