# ADR-0061: An app's data, its SQLite database included, stays in the platform's backup and device transfer on Android and iOS; a finance app such as Saldo is not an exception, and the only exclusion is the secure store (ADR-0037)

Status:        Accepted
Date decided:  2026-10-20
Deciding PRs:  #306
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/main/res/xml/{backup_rules,data_extraction_rules}.xml, demos/saldo/Android/app/src/main/res/xml/{backup_rules,data_extraction_rules}.xml, mobiler/plugins/sqlite/ios/SqlitePlugin.swift (`mobiler.db` in Documents)
Conformance:   xtask/tests/adr_conformance.rs::adr_0061_app_data_stays_in_backups

## 1. Context (The Problem)

ADR-0037 made the Android template exclude only the secure store from cloud backup and device transfer. It kept
`cx.save`'s state, SQLite databases and files, and rejected turning backup off. This record extends it; it does not
supersede it. ADR-0037 decided for Android only. iOS, where iCloud backs up the app's Documents folder (the sqlite
plugin keeps `mobiler.db` there), was never decided.

The security review of #305 raised it for Saldo, the finance showcase: its SQLite ledger goes into Google cloud
backup and phone-to-phone transfer, and a backup restored onto another device has no lock in front of it. The
maintainer first agreed to exclude the database, then asked (2026-10-20): "they can not backup someone else
database, so if they get it from their own account, should they just be able to use it?"

What keeping app data in backups exposes, stated plainly:
- **Someone who controls the user's Google account or Apple ID** can restore the backup onto their own phone and
  read the data. Saldo's biometric lock doesn't stop them: its setting travels with the data, but on their phone it
  accepts their fingerprint. The lock guards the app's screen, not its data.
- **iCloud backups are not end-to-end encrypted** unless the user turns on Advanced Data Protection, so Apple holds
  the keys. An unencrypted computer backup (Finder) also carries Documents. Android cloud backup is end-to-end
  encrypted when the phone has a screen lock (Android 9 and later).
- **On Android 8 to 11, someone holding the unlocked phone** can copy the database with `adb backup` (USB debugging
  on), past Saldo's lock.
- **Device transfer** needs the old phone in hand.

What excluding it costs: a user who never exported loses their whole history on a new, reset or replaced phone.

## 2. Hypothesis

For a personal, local-first app, losing data is the likelier harm. Every exposure above needs control of the user's
account or the unlocked phone in hand, and someone with either already reaches far more than one app's data. If app
data stays in backups on both platforms, users keep their history across phones without having to remember to
export.

### 2.1. Refutation Conditions

- **Condition 1: no app data is excluded on Android.** The template's and Saldo's `backup_rules.xml` and
  `data_extraction_rules.xml` exclude nothing but `mobiler_secure.xml`. They have no `<include>` rule (which would
  drop everything it doesn't name) and no encryption-only cloud backup, and both manifests allow backup. Saldo's rule
  files sit under `res/`, which ADR-0060's test leaves to the app, so this test checks them.
  - **Validation Metric:** `adr_0061_app_data_stays_in_backups`.
- **Condition 2: app data stays where backups reach on iOS and Android.** No template, bundled-plugin or Saldo
  source excludes a file from iCloud backup (`isExcludedFromBackup`, `NSURLIsExcludedFromBackupKey`) or keeps data in
  Android's `noBackupFilesDir`, and the sqlite plugin keeps `mobiler.db` in Documents (never Caches or tmp, which
  backups skip; caches like Saldo's photo captures may live there).
  - **Validation Metric:** the same test.

## 3. Considered Options & Rationale for Refutation

- **Option A: exclude the database for Saldo** `[recorded: the #305 security review; the maintainer, 2026-10-20: "yes, exclude it from backup in a later round"]`
  Rejected (the maintainer, 2026-10-20, after the risks above: "Keep backups"). Users who never export would lose
  their data. It would also be an edit to Saldo's own `res/xml`, which ADR-0060 allows but which no other app gets.
- **Option B: a framework option to exclude an app's database** `[reconstructed]`
  Not built. No app asks for it. An app that needs it can edit its own two rule files, as ADR-0037 says.
- **Option C: turn backup off (`allowBackup="false"`, `isExcludedFromBackup` on Documents)** `[reconstructed]`
  Rejected for the same reason ADR-0037 gave: local-first apps would lose their data on a new phone.

## 4. Decision & Rationale for Corroboration

Option: keep app data in backups on both platforms, as ADR-0037 does for Android, now stated for iOS and for the SQLite
database, with Saldo as the finance example. The risks in §1 are accepted knowingly.

**Mutation proof:** the decision was broken six times on purpose. The test failed each time; the file was restored,
and the test passed again.
- `<exclude domain="database" path="mobiler.db"/>` was added to Saldo's `<cloud-backup>`
  (`ADR-0061: demos/saldo/…/data_extraction_rules.xml excludes app data from backup: …`).
- `<exclude domain="sharedpref" path="mobiler.xml"/>` was added to the template's `backup_rules.xml`
  (`ADR-0061: mobiler/templates/…/backup_rules.xml excludes app data from backup: …`).
- The iOS sqlite plugin set `isExcludedFromBackup` (`ADR-0061: …/SqlitePlugin.swift excludes a file from iCloud backup`).

After the review, three more:
- A paired `<exclude domain="database" path="mobiler.db"></exclude>` in Saldo's `<cloud-backup>` (`… excludes app
  data from backup: …`). The first version of the test passed this one.
- `<include domain="sharedpref" path="."/>` in the template's `<device-transfer>` (`… has an include rule, which drops
  everything it doesn't name`).
- The iOS sqlite plugin's database moved to `.cachesDirectory` (`… must keep mobiler.db in Documents (backed up)`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a user's data survives a new phone, a reset or a restore, without an export.
- **Positive:** the question has a recorded answer with its real costs, for the next review that raises it.
- **Negative:** the exposures in §1 stand: account takeover, iCloud without Advanced Data Protection, `adb backup` on
  Android 8–11, a held unlocked phone. An app's own lock doesn't protect its data in any of them.
- **Negative:** an app that does need an exclusion edits its own rule files (Saldo's would be an edit ADR-0060 allows in
  `res/`, against this record). A framework-wide option would be a new record.
