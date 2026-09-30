# ADR-0033: Numbers, currency, dates and month and weekday names are formatted in the core by `mobiler_core::format` for a `Locale` the app passes; shells draw the resulting strings, with the exceptions listed in §5

Status:        Accepted
Date decided:  2026-06-04
Deciding PRs:  #107 (`mobiler_core::format`), #109 (`cx.device_locale`), #205 (`calendar_in`: the calendar's title, weekday header and week start move into the core)
Supersedes:    none
Code anchor:   mobiler-core/src/format.rs (Locale, Currency, Weekday, format_number, format_currency, format_date, month_year, weekday_short, Locale::from_tag, Locale::week_start), mobiler-core/src/lib.rs (calendar_in, Cx::device_locale), mobiler-ui/src/lib.rs (Widget::Calendar title/weekday_labels/leading_blanks)
Conformance:   mobiler-core/src/lib.rs::calendar_in_localizes_layout_and_clamps_markers, mobiler-core/src/format.rs::currency_placement

## 1. Context (The Problem)

The vacuro app needed Swiss amounts (`CHF 1'234.50`) and de/fr/it conventions (PR #107 is its
"Sub-batch B item #1"). Until then an app formatted money itself. The obvious tool, the platform's `NumberFormatter` / `Intl` /
`java.text`, lives in the shells. But `view()` builds the whole `Widget` tree synchronously, and a
shell capability answers only asynchronously (ADR-0002). So `view` cannot ask a shell to format a
number. The ABI's text widgets carry plain strings.

## 2. Hypothesis

If formatting is a pure, synchronous Rust module keyed by an explicit `Locale`, and widgets that
show localized dates (the calendar) receive finished strings from the core, then:

- `view` can format anything inline, with no request;
- the same amount or date reads the same on iOS, Android and web;
- the app, not the device, chooses the locale, and can change it at runtime.

### 2.1. Refutation Conditions

- **Condition 1 — the calendar arrives formatted.** For `Locale::SrLatn` the core puts
  "Septembar 2026" and the Monday-first header `P U S Č P S N` in the widget, and the leading blanks
  follow the locale's week start.
  - **Validation Metric:** `calendar_in_localizes_layout_and_clamps_markers` in
    `mobiler-core/src/lib.rs`.
- **Condition 2 — currency conventions come from the core's tables.** Symbol placement and
  separators follow the `Locale`.
  - **Validation Metric:** `currency_placement` in `mobiler-core/src/format.rs` (and its siblings
    `dates`, `serbian`, `ukrainian`).
- **Condition 3 — no shell formats app-visible dates or amounts.** Review; §5 lists the known
  exceptions.

## 3. Considered Options & Rationale for Refutation

- **Option A — a formatting request to the shell** `[recorded: PR #107 ("`view()` is synchronous but capabilities/plugins are async, so locale formatting must run in the core"); mobiler-core/src/format.rs ("locale formatting can't be delegated to the platform's `NumberFormatter`/`Intl` at render time")]`
  Impossible from `view`. The app would have to pre-format every value through a round trip and
  store the results in its model.
- **Option B — ICU data in the core (`icu4x` or similar)** `[recorded: PR #107 ("no ICU data bundle → small wasm"); mobiler-core/src/format.rs ("keeping the wasm web shell small")]`
  Complete and correct, but the data bundle ships in every app, including the web build.
- **Option C — each shell formats typed values in the ABI (a `Widget::Money`, a date field)** `[reconstructed]`
  Each platform would format with its own rules and the device locale, so the three shells could
  disagree, and a new locale would need changes in three shells.
- **Option D — a hand-rolled, synchronous `format` module in the core for the locales apps target** `[recorded: PR #107; docs/superpowers/plans/2026-09-17-large-touch-targets.md ("Locale-dependent calendar layout is computed in `mobiler-core` so shells only draw.")]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D. `mobiler_core::format` has `Locale` (`EnUs`, `EnGb`, `DeCh`, `FrCh`, `ItCh`, `DeDe`,
`FrFr`, `ItIt`, `UkUa`, `SrLatn`, `SrCyrl`) and `Currency` (`Chf`, `Eur`, `Usd`, `Gbp`, `Uah`,
`Rsd`), and returns `String`s for existing text widgets. #107 changed no ABI type. Every function
takes the `Locale` as an argument, so the app picks it. `cx.device_locale` (#109) returns the
device's BCP-47 tag through the built-in `device` capability, and `Locale::from_tag` maps it. The
saldo tutorial keeps two separate settings, UI language and regional format
(`docs/tutorial/saldo/07-go-multilingual.md`), which this allows.

#205 applied the rule to a widget. `Widget::Calendar` used to carry `first_weekday` and leave the
header to each shell. It now carries `title`, `weekday_labels` and `leading_blanks`, computed by
`calendar_in(locale, …)`. `calendar(…)` is `calendar_in(Locale::EnUs, …)` and renders as before.
Adding Ukrainian (#155) was one new variant and its tables in `format.rs`, with no shell change.

Shells still exchange dates in a fixed machine format: the date/time picker answers `yyyy-MM-dd` /
`HH:mm` (iOS `DateTimePlugin` uses `en_US_POSIX`), and the app formats them for display.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** one amount reads the same on all three shells, and a locale is added once, in Rust.
- **Positive:** the app's language and formatting locale can differ from the device's; the text
  shells draw themselves follows the same idea (ADR-0016).
- **Negative:** chart axis ticks are formatted in each shell, not the core: Android
  `"%.1f".format` (device locale, so `2,5`), iOS `String(format:)` and web `format!` (`2.5`).
  Whole-number ticks also differ: Android and iOS truncate, web rounds.
- **Negative:** Android's date/time picker builds its `yyyy-MM-dd` / `HH:mm` answer with
  `String.format` in the default locale (`Core.kt`), so a device in a locale with non-ASCII digits
  (`fa`, `ar`) can send non-ASCII digits in what should be a fixed machine format. iOS pins
  `en_US_POSIX`. Found while writing this record; not yet fixed.
- **Negative:** coverage is only the locales listed. Any other region needs a framework change and
  a `mobiler-core` release. The rules are hand-written, so they can be wrong where ICU would be
  right, and there is no plural, time-of-day, relative-date or unit formatting.
- **Negative:** chart axis ticks are still formatted in each shell (`fmtTick` on iOS and Android,
  `fmt_tick` on web). Android's `"%.1f".format(v)` uses the device locale, so a fractional tick
  can read `2,5` there and `2.5` on iOS and web.
- **Negative:** platform-drawn controls stay outside the rule. The native date/time pickers show
  month and day names in the device locale (as ADR-0016 notes for their labels).
- **Negative:** a widget that shows locale-dependent text must carry finished strings. The
  calendar's switch to `title` / `weekday_labels` was an ABI change for exhaustive matches and
  full literals (ADR-0008).
