//! Locale-aware number, currency, and date formatting — pure, synchronous, dependency-light.
//!
//! `view()` builds the `Widget` tree synchronously, but device capabilities/plugins are async, so
//! locale formatting can't be delegated to the platform's `NumberFormatter`/`Intl` at render time —
//! it has to run in the core. This module hand-rolls the conventions for the locales Mobiler apps
//! actually target (Swiss de/fr/it + the common Western locales) instead of pulling in a full ICU
//! data bundle, keeping the wasm web shell small.
//!
//! ```
//! use mobiler_core::format::{format_currency, Currency, Locale};
//! assert_eq!(format_currency(1234.5, Currency::Chf, Locale::DeCh), "CHF 1'234.50");
//! assert_eq!(format_currency(1234.5, Currency::Eur, Locale::DeDe), "1.234,50 €");
//! ```

use serde::{Deserialize, Serialize};

/// A formatting locale — drives digit grouping, the decimal mark, currency placement, and the
/// date order + month names. Map a device language tag (e.g. from a `locale` plugin) to one with
/// [`Locale::from_tag`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    EnUs,
    EnGb,
    /// Swiss German — `1'234.50`, `31.12.2026`.
    DeCh,
    /// Swiss French — `1'234.50`, `31.12.2026`.
    FrCh,
    /// Swiss Italian — `1'234.50`, `31.12.2026`.
    ItCh,
    DeDe,
    FrFr,
    ItIt,
    /// Serbian, Latin script — `1.234,50`, `31.12.2026.`, `januar`.
    SrLatn,
    /// Serbian, Cyrillic script — `1.234,50`, `31.12.2026.`, `јануар`.
    SrCyrl,
}

/// A currency. Placement (symbol leading vs trailing) follows the [`Locale`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Currency {
    Chf,
    Eur,
    Usd,
    Gbp,
    /// Serbian dinar — trails (`din.` in Latin, `дин.` in Cyrillic, else `RSD`).
    Rsd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    De,
    Fr,
    It,
    SrLatn,
    SrCyrl,
}

impl Locale {
    /// `(group separator, decimal separator)`.
    const fn seps(self) -> (&'static str, &'static str) {
        match self {
            Locale::EnUs | Locale::EnGb => (",", "."),
            // Swiss: apostrophe grouping, period decimal (e.g. SSM's "CHF 80'000").
            Locale::DeCh | Locale::FrCh | Locale::ItCh => ("'", "."),
            Locale::DeDe | Locale::ItIt | Locale::SrLatn | Locale::SrCyrl => (".", ","),
            // French: narrow no-break space grouping, comma decimal.
            Locale::FrFr => ("\u{202f}", ","),
        }
    }

    const fn lang(self) -> Lang {
        match self {
            Locale::EnUs | Locale::EnGb => Lang::En,
            Locale::DeCh | Locale::DeDe => Lang::De,
            Locale::FrCh | Locale::FrFr => Lang::Fr,
            Locale::ItCh | Locale::ItIt => Lang::It,
            Locale::SrLatn => Lang::SrLatn,
            Locale::SrCyrl => Lang::SrCyrl,
        }
    }

    /// Best-effort map of a BCP-47 language tag (case-insensitive, e.g. `"de-CH"`, `"fr"`,
    /// `"en-US"`) to a supported [`Locale`]. Region wins when known; otherwise the language's
    /// most common locale is used. Returns `None` for unrecognized languages.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Locale> {
        let t = tag.to_ascii_lowercase().replace('_', "-");
        let mut parts = t.split('-');
        let lang = parts.next().unwrap_or("");
        let region = parts.next().unwrap_or("");
        Some(match (lang, region) {
            ("en", "gb") => Locale::EnGb,
            ("en", _) => Locale::EnUs,
            ("de", "ch") => Locale::DeCh,
            ("de", _) => Locale::DeDe,
            ("fr", "ch") => Locale::FrCh,
            ("fr", _) => Locale::FrFr,
            ("it", "ch") => Locale::ItCh,
            ("it", _) => Locale::ItIt,
            // Serbian: script wins; default to Cyrillic (the official script) when unspecified.
            ("sr", "latn") => Locale::SrLatn,
            ("sr", _) => Locale::SrCyrl,
            _ => return None,
        })
    }
}

/// Group an unsigned integer's digit string with `sep` every three digits from the right.
fn group_digits(digits: &str, sep: &str) -> String {
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3 * sep.len());
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push_str(sep);
        }
        out.push(ch);
    }
    out
}

/// Format a floating-point number with a fixed number of decimal places, grouped per `locale`.
///
/// ```
/// use mobiler_core::format::{format_number, Locale};
/// assert_eq!(format_number(1234567.5, 2, Locale::DeCh), "1'234'567.50");
/// assert_eq!(format_number(1234567.5, 2, Locale::DeDe), "1.234.567,50");
/// ```
#[must_use]
pub fn format_number(value: f64, decimals: usize, locale: Locale) -> String {
    let (group, dec) = locale.seps();
    let s = format!("{:.*}", decimals, value.abs());
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s.as_str(), None),
    };
    // Determine sign from the rounded result so -0.00 reads as "0.00".
    let negative = value < 0.0 && s.bytes().any(|b| b != b'0' && b != b'.');
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&group_digits(int_part, group));
    if let Some(f) = frac_part {
        out.push_str(dec);
        out.push_str(f);
    }
    out
}

/// Format an integer, grouped per `locale`.
///
/// ```
/// use mobiler_core::format::{format_int, Locale};
/// assert_eq!(format_int(80000, Locale::DeCh), "80'000");
/// assert_eq!(format_int(-1234, Locale::EnUs), "-1,234");
/// ```
#[must_use]
pub fn format_int(value: i64, locale: Locale) -> String {
    let (group, _) = locale.seps();
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    if value < 0 {
        out.push('-');
    }
    out.push_str(&group_digits(&digits, group));
    out
}

/// Format a monetary amount (always two fraction digits). Symbol placement follows `locale`:
/// CHF/USD/GBP lead; EUR trails in de-DE/fr-FR/it-IT and leads elsewhere.
///
/// ```
/// use mobiler_core::format::{format_currency, Currency, Locale};
/// assert_eq!(format_currency(1234.5, Currency::Chf, Locale::FrCh), "CHF 1'234.50");
/// assert_eq!(format_currency(1234.5, Currency::Usd, Locale::EnUs), "$1,234.50");
/// ```
#[must_use]
pub fn format_currency(value: f64, currency: Currency, locale: Locale) -> String {
    let num = format_number(value, 2, locale);
    match currency {
        Currency::Chf => format!("CHF {num}"),
        Currency::Usd => format!("${num}"),
        Currency::Gbp => format!("£{num}"),
        Currency::Eur => match locale {
            Locale::DeDe | Locale::FrFr | Locale::ItIt => format!("{num} €"),
            _ => format!("€{num}"),
        },
        Currency::Rsd => match locale {
            Locale::SrLatn => format!("{num} din."),
            Locale::SrCyrl => format!("{num} дин."),
            _ => format!("{num} RSD"),
        },
    }
}

const MONTHS_EN: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
const MONTHS_DE: [&str; 12] = [
    "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober",
    "November", "Dezember",
];
const MONTHS_FR: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre",
    "octobre", "novembre", "décembre",
];
const MONTHS_IT: [&str; 12] = [
    "gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre",
    "ottobre", "novembre", "dicembre",
];
const MONTHS_SR_LATN: [&str; 12] = [
    "januar", "februar", "mart", "april", "maj", "jun", "jul", "avgust", "septembar", "oktobar",
    "novembar", "decembar",
];
const MONTHS_SR_CYRL: [&str; 12] = [
    "јануар", "фебруар", "март", "април", "мај", "јун", "јул", "август", "септембар", "октобар",
    "новембар", "децембар",
];

/// The localized full month name (`month` is 1–12, clamped).
#[must_use]
pub fn month_name(month: u32, locale: Locale) -> &'static str {
    let idx = (month.clamp(1, 12) - 1) as usize;
    match locale.lang() {
        Lang::En => MONTHS_EN[idx],
        Lang::De => MONTHS_DE[idx],
        Lang::Fr => MONTHS_FR[idx],
        Lang::It => MONTHS_IT[idx],
        Lang::SrLatn => MONTHS_SR_LATN[idx],
        Lang::SrCyrl => MONTHS_SR_CYRL[idx],
    }
}

/// Numeric date in the locale's conventional order/separator.
///
/// ```
/// use mobiler_core::format::{format_date, Locale};
/// assert_eq!(format_date(2026, 1, 5, Locale::DeCh), "05.01.2026");
/// assert_eq!(format_date(2026, 1, 5, Locale::EnUs), "01/05/2026");
/// ```
#[must_use]
pub fn format_date(year: i32, month: u32, day: u32, locale: Locale) -> String {
    match locale {
        Locale::EnUs => format!("{month:02}/{day:02}/{year}"),
        Locale::EnGb | Locale::FrFr | Locale::ItIt => format!("{day:02}/{month:02}/{year}"),
        Locale::DeCh | Locale::FrCh | Locale::ItCh | Locale::DeDe => {
            format!("{day:02}.{month:02}.{year}")
        }
        // Serbian uses a trailing dot: "31.12.2026."
        Locale::SrLatn | Locale::SrCyrl => format!("{day:02}.{month:02}.{year}."),
    }
}

/// Long date with the localized month name (e.g. `"5. Januar 2026"`, `"January 5, 2026"`).
///
/// ```
/// use mobiler_core::format::{format_date_long, Locale};
/// assert_eq!(format_date_long(2026, 1, 5, Locale::DeCh), "5. Januar 2026");
/// assert_eq!(format_date_long(2026, 1, 5, Locale::EnUs), "January 5, 2026");
/// ```
#[must_use]
pub fn format_date_long(year: i32, month: u32, day: u32, locale: Locale) -> String {
    let m = month_name(month, locale);
    match locale.lang() {
        Lang::En => match locale {
            Locale::EnUs => format!("{m} {day}, {year}"),
            _ => format!("{day} {m} {year}"),
        },
        // German + Serbian use the ordinal dot after the day; French/Italian do not.
        Lang::De | Lang::SrLatn | Lang::SrCyrl => format!("{day}. {m} {year}"),
        Lang::Fr | Lang::It => format!("{day} {m} {year}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swiss_grouping_uses_apostrophe() {
        assert_eq!(format_int(80000, Locale::DeCh), "80'000");
        assert_eq!(format_int(1234567, Locale::FrCh), "1'234'567");
        assert_eq!(format_number(1234567.5, 2, Locale::ItCh), "1'234'567.50");
    }

    #[test]
    fn western_number_conventions() {
        assert_eq!(format_number(1234567.5, 2, Locale::EnUs), "1,234,567.50");
        assert_eq!(format_number(1234567.5, 2, Locale::DeDe), "1.234.567,50");
        assert_eq!(format_number(1234.5, 2, Locale::FrFr), "1\u{202f}234,50");
        assert_eq!(format_number(12.0, 0, Locale::EnUs), "12");
        assert_eq!(format_number(999.999, 2, Locale::EnUs), "1,000.00"); // rounds up + regroups
    }

    #[test]
    fn negatives_and_zero() {
        assert_eq!(format_int(-1234, Locale::EnUs), "-1,234");
        assert_eq!(format_number(-0.001, 2, Locale::EnUs), "0.00"); // rounds to zero → no sign
        assert_eq!(format_number(-12.5, 1, Locale::DeCh), "-12.5");
    }

    #[test]
    fn currency_placement() {
        assert_eq!(format_currency(1234.5, Currency::Chf, Locale::DeCh), "CHF 1'234.50");
        assert_eq!(format_currency(1234.5, Currency::Usd, Locale::EnUs), "$1,234.50");
        assert_eq!(format_currency(1234.5, Currency::Gbp, Locale::EnGb), "£1,234.50");
        assert_eq!(format_currency(1234.5, Currency::Eur, Locale::DeDe), "1.234,50 €");
        assert_eq!(format_currency(1234.5, Currency::Eur, Locale::EnUs), "€1,234.50");
    }

    #[test]
    fn dates() {
        assert_eq!(format_date(2026, 1, 5, Locale::DeCh), "05.01.2026");
        assert_eq!(format_date(2026, 1, 5, Locale::EnUs), "01/05/2026");
        assert_eq!(format_date(2026, 12, 31, Locale::ItIt), "31/12/2026");
        assert_eq!(format_date_long(2026, 1, 5, Locale::DeCh), "5. Januar 2026");
        assert_eq!(format_date_long(2026, 3, 5, Locale::FrCh), "5 mars 2026");
        assert_eq!(format_date_long(2026, 1, 5, Locale::EnUs), "January 5, 2026");
    }

    #[test]
    fn serbian() {
        // Latin + Cyrillic share number/date conventions (".", ",", trailing-dot date).
        assert_eq!(format_int(1234567, Locale::SrLatn), "1.234.567");
        assert_eq!(format_number(1234.5, 2, Locale::SrCyrl), "1.234,50");
        assert_eq!(format_date(2026, 12, 31, Locale::SrLatn), "31.12.2026.");
        assert_eq!(format_currency(1234.5, Currency::Rsd, Locale::SrLatn), "1.234,50 din.");
        assert_eq!(format_currency(1234.5, Currency::Rsd, Locale::SrCyrl), "1.234,50 дин.");
        assert_eq!(format_currency(1234.5, Currency::Rsd, Locale::EnUs), "1,234.50 RSD");
        // Month names differ by script.
        assert_eq!(format_date_long(2026, 1, 5, Locale::SrLatn), "5. januar 2026");
        assert_eq!(format_date_long(2026, 1, 5, Locale::SrCyrl), "5. јануар 2026");
    }

    #[test]
    fn tag_parsing() {
        assert_eq!(Locale::from_tag("de-CH"), Some(Locale::DeCh));
        assert_eq!(Locale::from_tag("fr_FR"), Some(Locale::FrFr));
        assert_eq!(Locale::from_tag("EN-us"), Some(Locale::EnUs));
        assert_eq!(Locale::from_tag("it"), Some(Locale::ItIt));
        assert_eq!(Locale::from_tag("sr-Latn-RS"), Some(Locale::SrLatn));
        assert_eq!(Locale::from_tag("sr"), Some(Locale::SrCyrl));
        assert_eq!(Locale::from_tag("sr-RS"), Some(Locale::SrCyrl));
        assert_eq!(Locale::from_tag("ja-JP"), None);
    }
}
