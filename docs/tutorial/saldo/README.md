# Build a real app: **Saldo**

A hands-on, chapter-by-chapter tutorial that builds **Saldo** — a multilingual, SQLite-backed
personal expense & money manager — from an empty scaffold to a store-ready app, entirely in Rust on
[Mobiler](../../../README.md). Every chapter is a working increment with the exact code, and the
finished app lives in [`demos/saldo/`](../../../demos/saldo). Its native shells are exactly what `mobiler new` and
`mobiler plugin add` produce, kept current with `mobiler upgrade` on every CLI release (ADR-0060).

Saldo is a real, full-featured money manager — a ledger of **income / expense / transfer** entries
across multiple **accounts**, organized by **categories**, summarized with **charts**, exportable to
**CSV**, localized into **English, German, French, Italian and Ukrainian**, and locked behind a
**passcode**. We build it on the framework's existing capabilities: the `sqlite`, `files`,
`filepicker` and `biometric` plugins, the `Chart` widget, `mobiler_core::format`, and the
`mobiler_core::i18n` translation primitive. Saldo is the app that added `i18n`, `cx.now()` and Ukrainian
formatting to the framework; all three shipped in `mobiler-core` 0.31, so on today's framework you just
use them. The chapters say where each one came from.

## Chapters

1. **[Scaffold & first screen](01-scaffold-and-first-screen.md)** — `mobiler new`, the mobile-only
   project layout, the `MobilerApp` (Model / Msg / `update` / `input` / `view`), the four-tab
   `scaffold`, a floating action button, a bottom sheet, and an in-memory expense ledger.
2. **[Persist with SQLite](02-persist-with-sqlite.md)** — the schema, creating it on launch,
   insert/query with bound params, the `cx.now()` capability (added for Saldo), day-grouping, period filters, and the
   "the Model isn't persisted — re-read it on launch" pattern.
3. **[Accounts & transfers](03-accounts-and-transfers.md)** — a unified income/expense/transfer
   ledger across accounts, balances + net worth on the Assets tab, and a `PRAGMA user_version` schema
   migration that carries the Chapter-2 data forward.
4. **[Categories & the entry sheet](04-categories-and-the-entry-sheet.md)** — a seeded category tree
   (income/expense, two levels), a hierarchical category picker, and a native date picker for
   back-dating (`cx.pick_date`).
5. **[Stats](05-stats.md)** — a category-breakdown donut (income/expense toggle, per period) with a
   ranked list, a net-worth trend line, and a monthly income-vs-expense bar chart — all from the
   `Chart` widget over pure aggregation helpers.
6. **[Money & formatting](06-money-and-formatting.md)** — route every amount and date through
   `mobiler_core::format`, pick the locale from the device (`cx.device_locale`), and add **Ukrainian**
   (`Locale::UkUa` / `Currency::Uah`) to the framework (in `mobiler-core` since 0.31).
7. **[Go multilingual](07-go-multilingual.md)** — `mobiler_core::i18n` (since `mobiler-core` 0.31; language negotiation +
   a fallback-aware `Catalog`): translate every string into en/de/fr/it/uk, pick the language from the
   device, and let the user override it in Settings.
8. **[Recurring transactions](08-recurring.md)** — a `recurring` rules table, a "Repeat" choice on the
   entry sheet, and catch-up materialization on launch (dependency-free civil-date arithmetic).
9. **[Export & backup](09-export-import-backup.md)** — a CSV export of the ledger and a full JSON
   backup/restore (the `files` + `filepicker` plugins), from a Data card in Settings.
10. **[Security, settings & polish](10-security-settings-polish.md)** — an optional biometric app lock
    (`biometric`, device-passcode fallback), the Saldo brand theme (`with_theme`) with a light/dark
    toggle, and a real app icon.

> The look is intentionally clean and modern — Mobiler renders the same Rust `Widget` tree to native
> SwiftUI, Jetpack Compose, and the web, so we describe *what* the UI is and the platforms draw it.
