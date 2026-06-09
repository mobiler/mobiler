# Build a real app: **Saldo**

A hands-on, chapter-by-chapter tutorial that builds **Saldo** — a multilingual, SQLite-backed
personal expense & money manager — from an empty scaffold to a store-ready app, entirely in Rust on
[Mobiler](../../../README.md). Every chapter is a working increment with the exact code, and the
finished app lives in [`demos/saldo/`](../../../demos/saldo).

Saldo is a real, full-featured money manager — a ledger of **income / expense / transfer** entries
across multiple **accounts**, organized by **categories**, summarized with **charts**, exportable to
**CSV**, localized into **English, German, French, Italian and Ukrainian**, and locked behind a
**passcode**. We build it on the framework's existing capabilities: the `sqlite`, `files`,
`filepicker`, `biometric` and `securestore` plugins, the `Chart` widget, `mobiler_core::format`, and a
new `mobiler_core::i18n` translation primitive we add along the way.

## Chapters

1. **[Scaffold & first screen](01-scaffold-and-first-screen.md)** — `mobiler new`, the mobile-only
   project layout, the `MobilerApp` (Model / Msg / `update` / `input` / `view`), the four-tab
   `scaffold`, a floating action button, a bottom sheet, and an in-memory expense ledger.
2. **[Persist with SQLite](02-persist-with-sqlite.md)** — the schema, creating it on launch,
   insert/query with bound params, a new `cx.now()` capability, day-grouping, period filters, and the
   "the Model isn't persisted — re-read it on launch" pattern.
3. **Accounts & transfers** — the Assets tab: accounts, balances, net worth, transfers.
4. **Categories & the entry sheet** — editable income/expense categories with subcategories, the date
   picker, the category picker.
5. **Stats** — pie/donut by category, a net-worth trend line, monthly bars (the `Chart` widget).
6. **Money & formatting** — locale-aware currency and dates with `mobiler_core::format` (and adding
   Ukrainian to the framework).
7. **Go multilingual** — the new `mobiler_core::i18n`: negotiate the device language, translate every
   string, let the user override.
8. **Recurring transactions** — schedule rules and materialize due entries on launch.
9. **Export, import & backup** — CSV export, CSV import, and full JSON backup/restore.
10. **Security, settings & polish** — passcode/biometric lock, settings, theming, icons, dark mode.

> The look is intentionally clean and modern — Mobiler renders the same Rust `Widget` tree to native
> SwiftUI, Jetpack Compose, and the web, so we describe *what* the UI is and the platforms draw it.
