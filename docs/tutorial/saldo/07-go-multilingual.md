# Chapter 7 — Go multilingual

Chapter 6 made *numbers and dates* locale-aware, but every **label** is still English. This chapter makes
Saldo speak five languages — English, German, French, Italian and Ukrainian — by adding a small
translation primitive to the framework, `mobiler_core::i18n`, building a catalog of every UI string, and
picking the language from the device (with a manual override in Settings).

## 1. The `i18n` primitive

Like `format`, translation has to run *in the core*: `view()` is synchronous, so it can't ask the
platform for strings at render time. The new module gives the app two tiny pieces — **language
negotiation** and a **catalog** — and lets the app supply its own languages and strings:

```rust
use mobiler_core::i18n::{negotiate, Catalog};

// Pick the best supported language for a device tag (language-subtag match, case-insensitive).
negotiate("de-CH", &["en", "de", "fr", "it", "uk"], "en"); // "de"
negotiate("ja-JP", &["en", "de"], "en");                    // "en" — unsupported → default

let cat = Catalog::new("en").with("save", &[("en", "Save"), ("uk", "Зберегти")]);
cat.tr("save", "uk"); // "Зберегти"
cat.tr("save", "fr"); // "Save"  — missing language → the default language
cat.tr("x", "uk");    // "x"     — missing key → the key itself
```

The two-step fallback (requested language → default language → the key) means a missing translation
always degrades to *something* readable instead of a blank.

## 2. Negotiating the language at startup

We already fetch the device locale in Chapter 6; now we also derive the UI language from it. The model
keeps the negotiated language and an optional manual override:

```rust
const SUPPORTED: [&str; 5] = ["en", "de", "fr", "it", "uk"];

Msg::GotLocale(tag) => {
    // …locale + currency from Chapter 6…
    model.device_lang = negotiate(&tag, &SUPPORTED, "en");
}
```

```rust
fn lang(model: &Model) -> &str {
    model.lang_override.as_deref().unwrap_or(&model.device_lang)
}
```

## 3. One catalog, built once

The catalog is pure data, so we build it a single time behind a `OnceLock` and read it on every render.
English is the default; each key lists its translations:

```rust
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        Catalog::new("en")
            .with("tab.bills", &[("en", "Bills"), ("de", "Buchungen"), ("fr", "Opérations"),
                                 ("it", "Movimenti"), ("uk", "Операції")])
            .with("action.save", &[("en", "Save"), ("de", "Speichern"), ("fr", "Enregistrer"),
                                   ("it", "Salva"), ("uk", "Зберегти")])
            // …~45 keys: tabs, periods, the ledger, stats, both sheets, settings, errors…
    })
}
```

A thin helper translates a key in the model's current language, and that's what the views call:

```rust
fn tr(model: &Model, key: &'static str) -> String {
    catalog().tr(key, lang(model)).to_string()
}
```

> We name it `tr`, not `t` — `t` is already the loop variable for a transaction in a couple of view
> functions, and shadowing it would be a trap.

## 4. Translating the views

Every user-facing literal becomes a `tr(model, …)` call. Builders take `impl Into<String>`, so the
returned `String` drops straight in:

```rust
row(vec![caption(tr(model, "income")),  spacer(Spacing::Md), emphasis(money(model, income))]),
row(vec![caption(tr(model, "expense")), spacer(Spacing::Md), emphasis(money(model, expense))]),
```

Chart series names are strings too, so the legends localize for free:

```rust
chart(vec![ChartSeries::new(tr(model, "income"), inc), ChartSeries::new(tr(model, "expense"), exp)], …)
```

**What we don't translate is user data** — account names like *Cash* and category names like *Groceries*
are rows in the database the user can rename, not chrome. Only the framework-drawn labels go through the
catalog.

## 5. Errors are keys, not English

Validation already returned a message string; now it returns a **key**, and the caller localizes it when
it shows the toast — so the same validation logic produces a German or Ukrainian message with no extra
branching:

```rust
fn validate_txn(model: &Model) -> Result<(), &'static str> {
    if parse_amount(&model.draft_amount).is_none() { return Err("err.amount"); }
    // …
}

// at the call site:
if let Err(why) = validate_txn(model) {
    cx.notify("toast", "show", tr(model, why));
    return;
}
```

## 6. A language override in Settings

The Settings tab — until now a placeholder — gets a language picker. A **System** chip clears the
override (follow the device); each language chip is labelled with its own endonym (*Deutsch*,
*Українська*) so anyone can find their language:

```rust
fn settings(model: &Model) -> Widget {
    let chosen = model.lang_override.as_deref();
    let mut chips = vec![chip(tr(model, "settings.system"), chosen.is_none(), Msg::SetLang(None))];
    for code in SUPPORTED {
        let label = match code { "de" => "Deutsch", "fr" => "Français", "it" => "Italiano",
                                  "uk" => "Українська", _ => "English" };
        chips.push(chip(label, chosen == Some(code), Msg::SetLang(Some(code.to_string()))));
    }
    // …a card with the "Language" title + the chip row…
}
```

```rust
Msg::SetLang(choice) => model.lang_override = choice,
```

Tapping a chip re-renders the whole app in the new language instantly — the catalog is already in memory,
so there's no reload. (Number and date formatting still follow the device region from Chapter 6; the
language and the regional format are independent settings, which is what most money managers do.)

## What we built

Saldo is now fully multilingual: a reusable `mobiler_core::i18n` primitive (negotiation + a
fallback-aware catalog), every string translated into en/de/fr/it/uk, the language chosen from the device
and overridable in Settings — covered by tests in the framework (`i18n::tests`) and the app (the override
path, plus a guard that *every* catalog key has all five languages).

`mobiler_core::i18n` and the Chapter-6 Ukrainian `format` support are the new framework surface this app
justified; they ship to other apps in the next `mobiler-core` release.

**Next:** Chapter 8 — Recurring transactions: schedule rules and post the due ones on launch.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs)
> and [`mobiler-core/src/i18n.rs`](../../../mobiler-core/src/i18n.rs).
