---
title: Try Saldo
description: Saldo, the money manager the tutorial builds, on your own phone.
---

Saldo is a real app built with Mobiler, and the [tutorial](/tutorial/) builds it chapter by chapter.
It keeps a ledger of income, expenses and transfers across your accounts, sorts them into categories,
shows where the money goes with charts, posts recurring entries by itself, exports to CSV, backs up and
restores, speaks English, German, French, Italian and Ukrainian, and can lock behind Face ID or your
fingerprint. There is no account and no server: your data lives on your phone and in its normal
iCloud or Google backup.

:::note[Coming soon to the App Store and Google Play]
Saldo is in testing now. The store links will be here as soon as it is published.
:::

## Build it yourself

Saldo is open source. With the [tools installed](/start/get-started/#what-you-need):

```sh
git clone https://github.com/mobiler/mobiler
cd mobiler/demos/saldo
mobiler dev              # Android, on your emulator or phone
mobiler build ios        # iOS, on a Mac
```

## What to look at

- **One core, no native code.** All of Saldo is
  [`shared/src/app.rs`](https://github.com/mobiler/mobiler/blob/main/demos/saldo/shared/src/app.rs). Its
  Android and iOS shells are exactly what `mobiler new` and `mobiler plugin add` produce.
- **Plugins:** `sqlite` for the ledger, `files` and `filepicker` for export and restore, `biometric` for the
  lock, `picker` for choices.
- **Its own look** from data: a full light and dark palette, finance icons, and iOS's floating glass tab bar.
