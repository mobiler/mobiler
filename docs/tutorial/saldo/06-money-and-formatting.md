# Chapter 6 — Money & formatting

Until now Saldo has formatted money with a hard-coded `format!("€{v:.2}")` and shown dates as the raw
`"YYYY-MM-DD"` strings from the database. A money manager that ships in five languages can't do that: a
Swiss user expects `CHF 1'234.50`, a German `1.234,50 €`, a Ukrainian `1 234,50 ₴` and `31.12.2026`. This
chapter routes every amount and date through **`mobiler_core::format`**, picks the right locale from the
**device** at startup, and — because Saldo targets Ukrainian — **adds Ukrainian to the framework** itself.

## 1. The `format` module

`mobiler_core::format` is a small, dependency-light, *synchronous* formatter. It has to be synchronous:
`view()` builds the widget tree without `await`, so it can't call the platform's `Intl`/`NumberFormatter`
at render time — the conventions are hand-rolled in the core instead. Everything is keyed by a `Locale`:

```rust
use mobiler_core::format::{format_currency, format_date, Currency, Locale};

format_currency(1234.5, Currency::Chf, Locale::DeCh); // "CHF 1'234.50"
format_currency(1234.5, Currency::Eur, Locale::DeDe); // "1.234,50 €"
format_date(2026, 12, 31, Locale::EnUs);              // "12/31/2026"
```

The locale drives digit grouping, the decimal mark, the currency symbol's placement, and the date order.

## 2. Adding Ukrainian to the framework

Ukrainian wasn't one of the module's locales, so we add it — a small, **additive** change to
`mobiler-core/src/format.rs` (a new enum variant breaks nothing, since the module's own `match`es are the
only exhaustive ones and we update them):

- `Locale::UkUa` — no-break-space digit grouping (`1 234`), comma decimal, `dd.MM.yyyy` dates, and a set
  of Ukrainian month names.
- `Currency::Uah` — the hryvnia, trailing the amount: `1 234,50 ₴`.
- `Locale::from_tag("uk")` now resolves to `UkUa`.

We also give `Locale` and `Currency` a `#[default]` (en-US / EUR) so the app can hold them in a
`#[derive(Default)]` model. The behaviour is locked down with unit tests right next to the others:

```rust
assert_eq!(format_currency(1234.5, Currency::Uah, Locale::UkUa), "1\u{a0}234,50 ₴");
assert_eq!(format_date(2026, 12, 31, Locale::UkUa), "31.12.2026");
assert_eq!(Locale::from_tag("uk-UA"), Some(Locale::UkUa));
```

Because Saldo depends on `mobiler-core` by path, it picks this up immediately — no publish needed yet
(the core release that ships this to *other* apps rides along with the `i18n` primitive in Chapter 7).

## 3. Picking the locale from the device

Currency and date conventions should match the phone's region, so we read the device locale once at
startup with the built-in `cx.device_locale` capability and map it with `Locale::from_tag`:

```rust
fn init(&self, _model: &mut Model, cx: &mut Cx<Msg>) {
    // …schema + cx.now…
    cx.device_locale(|r| Msg::GotLocale(r.output)); // r.output is a BCP-47 tag, e.g. "uk-UA"
}

Msg::GotLocale(tag) => {
    if let Some(loc) = Locale::from_tag(&tag) {
        model.locale = loc;
        model.currency = default_currency(loc); // a sensible region default; user-settable later
    }
}
```

The base currency is a real user setting (it lands in the Settings screen in Chapter 10); until then we
default it from the locale — hryvnia for Ukrainian, francs for Switzerland, dollars for the US, euros
otherwise — so the very first launch already looks right:

```rust
fn default_currency(locale: Locale) -> Currency {
    match locale {
        Locale::UkUa => Currency::Uah,
        Locale::EnUs => Currency::Usd,
        Locale::EnGb => Currency::Gbp,
        Locale::DeCh | Locale::FrCh | Locale::ItCh => Currency::Chf,
        _ => Currency::Eur,
    }
}
```

## 4. Formatting throughout the app

`money` now formats through the module, so it needs the model (for the locale + currency). The rest of the
view code already has the model in hand, so threading it through is mechanical:

```rust
fn money(model: &Model, v: f64) -> String {
    format_currency(v, model.currency, model.locale)
}
```

Dates get the same treatment — one helper parses the stored `"YYYY-MM-DD"` and formats it, falling back to
the raw string if it can't parse (so a malformed value never panics or shows blank):

```rust
fn fmt_date(model: &Model, ymd: &str) -> String {
    match parse_ymd(ymd) {
        Some((y, m, d)) => format_date(y, m, d, model.locale),
        None => ymd.to_string(),
    }
}
```

The day-group headers on the Bills tab and the date row in the entry sheet now read `fmt_date(model, …)`
instead of the raw string, and every `money(income)` becomes `money(model, income)`.

## 5. What we built

Saldo now speaks money and dates in the device's locale — Swiss apostrophes, German trailing euros,
Ukrainian hryvnia and `dd.MM.yyyy` — by adding **Ukrainian** to `mobiler_core::format` and routing all
formatting through it, with the locale negotiated from `cx.device_locale` at launch. The logic is covered
by tests both in the framework (`format::tests::ukrainian`) and the app (`default_currency`, `fmt_date`).

What's still in English is every *label* — "Income", "Expense", "By category". That's the next chapter.

**Next:** Chapter 7 — Go multilingual: the new `mobiler_core::i18n` primitive (negotiate the device
language, translate every string into en/de/fr/it/uk, and let the user override it).

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs)
> and [`mobiler-core/src/format.rs`](../../../mobiler-core/src/format.rs).
