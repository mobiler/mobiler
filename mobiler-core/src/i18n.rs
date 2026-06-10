//! Tiny, dependency-free localization: pick a UI language from the device, then translate keys.
//!
//! `view()` is synchronous, so translations can't be fetched from the platform at render time — they
//! live in the core as a small in-memory [`Catalog`] the app builds from string literals (typically
//! once, behind a `OnceLock`) and reads while rendering. The framework supplies the machinery
//! ([`negotiate`] + [`Catalog`]); the app supplies its own languages and strings. Pair it with
//! [`Cx::device_locale`](crate::Cx::device_locale) at startup and the
//! [`format`](crate::format) module for numbers/dates.
//!
//! ```
//! use mobiler_core::i18n::{negotiate, Catalog};
//!
//! // Choose the UI language from the device tag, against the languages the app ships.
//! let lang = negotiate("de-CH", &["en", "de", "fr", "it", "uk"], "en");
//! assert_eq!(lang, "de");
//!
//! let cat = Catalog::new("en")
//!     .with("save", &[("en", "Save"), ("de", "Speichern"), ("uk", "Зберегти")]);
//! assert_eq!(cat.tr("save", &lang), "Speichern");
//! assert_eq!(cat.tr("save", "fr"), "Save"); // missing language → the default language
//! assert_eq!(cat.tr("undefined", "de"), "undefined"); // missing key → the key itself
//! ```

use std::collections::BTreeMap;

/// Pick the best-matching language **code** from `supported` for a device BCP-47 `tag`, comparing on
/// the language subtag only (case-insensitive); falls back to `default`. Returns an owned code so it
/// can be stored in the model and handed to [`Catalog::tr`].
///
/// ```
/// use mobiler_core::i18n::negotiate;
/// assert_eq!(negotiate("uk-UA", &["en", "uk"], "en"), "uk");
/// assert_eq!(negotiate("EN", &["en", "de"], "en"), "en"); // case-insensitive
/// assert_eq!(negotiate("ja-JP", &["en", "de"], "en"), "en"); // unsupported → default
/// assert_eq!(negotiate("", &["en"], "en"), "en");
/// ```
#[must_use]
pub fn negotiate(tag: &str, supported: &[&str], default: &str) -> String {
    let lang = tag.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    supported
        .iter()
        .find(|s| s.eq_ignore_ascii_case(&lang))
        .map_or_else(|| default.to_string(), |s| (*s).to_string())
}

/// An app-populated translation table: `key → (language code → string)`, with a default language used
/// as a fallback. Build it once from string literals (e.g. behind a `OnceLock`) and read it in `view`.
///
/// Lookups fall back in two steps — the requested language, then the default language, then the key
/// itself — so a missing translation degrades to *something* readable rather than blank.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    default: String,
    entries: BTreeMap<&'static str, BTreeMap<&'static str, &'static str>>,
}

impl Catalog {
    /// A new, empty catalog whose `default_lang` is the fallback when a requested language is missing.
    #[must_use]
    pub fn new(default_lang: &str) -> Self {
        Self { default: default_lang.to_string(), entries: BTreeMap::new() }
    }

    /// Add (or extend) the translations for `key`. Chainable, for building the catalog inline.
    ///
    /// ```
    /// use mobiler_core::i18n::Catalog;
    /// let c = Catalog::new("en")
    ///     .with("hello", &[("en", "Hello"), ("uk", "Привіт")])
    ///     .with("bye", &[("en", "Bye"), ("uk", "Бувай")]);
    /// assert_eq!(c.tr("hello", "uk"), "Привіт");
    /// ```
    #[must_use]
    pub fn with(mut self, key: &'static str, langs: &[(&'static str, &'static str)]) -> Self {
        self.entries.entry(key).or_default().extend(langs.iter().copied());
        self
    }

    /// Translate `key` into `lang`, falling back to the default language, then to `key` itself.
    #[must_use]
    pub fn tr<'a>(&'a self, key: &'a str, lang: &str) -> &'a str {
        let Some(by_lang) = self.entries.get(key) else { return key };
        by_lang.get(lang).or_else(|| by_lang.get(self.default.as_str())).copied().unwrap_or(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUPPORTED: [&str; 5] = ["en", "de", "fr", "it", "uk"];

    #[test]
    fn negotiate_matches_language_subtag() {
        assert_eq!(negotiate("de-CH", &SUPPORTED, "en"), "de");
        assert_eq!(negotiate("uk_UA", &SUPPORTED, "en"), "uk"); // underscore form
        assert_eq!(negotiate("FR", &SUPPORTED, "en"), "fr"); // case-insensitive, no region
        assert_eq!(negotiate("pt-BR", &SUPPORTED, "en"), "en"); // unsupported → default
        assert_eq!(negotiate("", &SUPPORTED, "en"), "en");
    }

    #[test]
    fn tr_falls_back_language_then_key() {
        let cat = Catalog::new("en")
            .with("ok", &[("en", "OK"), ("de", "OK"), ("uk", "Гаразд")])
            .with("save", &[("en", "Save"), ("uk", "Зберегти")]);
        assert_eq!(cat.tr("save", "uk"), "Зберегти"); // exact
        assert_eq!(cat.tr("save", "de"), "Save"); // missing language → default language
        assert_eq!(cat.tr("save", ""), "Save"); // empty language → default language
        assert_eq!(cat.tr("missing", "uk"), "missing"); // missing key → key itself
        assert_eq!(cat.tr("ok", "uk"), "Гаразд");
    }

    #[test]
    fn with_extends_an_existing_key() {
        let cat = Catalog::new("en").with("x", &[("en", "X")]).with("x", &[("de", "X-de")]);
        assert_eq!(cat.tr("x", "en"), "X");
        assert_eq!(cat.tr("x", "de"), "X-de");
    }
}
