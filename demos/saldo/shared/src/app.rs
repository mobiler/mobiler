//! Saldo — a multilingual, SQLite-backed personal expense & money manager, built on Mobiler.
//!
//! Grows over the tutorial (`docs/tutorial/saldo/`). **Chapter 10** is security & polish: an optional
//! **biometric app lock** (`biometric` plugin, device-passcode fallback), the Saldo **brand theme**
//! (`with_theme`) plus a light/dark toggle, and a real app icon. Earlier chapters set up the four-tab
//! shell, `SQLite` persistence with `cx.now()`, the accounts / transfers ledger, categories, the entry
//! sheet, the Stats charts, locale-aware money/date formatting, the multilingual `i18n` UI, recurring
//! transactions, and CSV/JSON export & backup. This is the last chapter — Saldo is feature-complete.

use std::sync::OnceLock;

use mobiler_core::format::{format_currency, format_date};
use mobiler_core::{
    ButtonStyle, CardStyle, Catalog, ChartSeries, ChartStyle, Corner, Currency, Cx, Density, FontFamily,
    Icon, InputValue, Locale, MobilerApp, MobilerShell, Rgb, Segment, Spacing, Theme, Tone, Widget,
    badge, button, caption, card, card_button, chart, chip, column, decimal_field, divider,
    donut_chart, emphasis, negotiate, row, scaffold, scroller, segment, segmented, spacer, subtitle,
    swipe_action, text, text_field, with_fab, with_sheet, with_theme,
};
use serde::{Deserialize, Serialize};

/// The UI languages Saldo ships (English is the base / fallback).
const SUPPORTED: [&str; 5] = ["en", "de", "fr", "it", "uk"];

// ---- schema + migrations (run once, gated by PRAGMA user_version) ----

const SCHEMA_VERSION: u32 = 5;

/// Statements to bring a database at version `from` up to `SCHEMA_VERSION`. `user_version` gates them
/// so they run exactly once; statements run in order (the queue drains one at a time), so later
/// statements may rely on earlier ones (e.g. subcategory seeds reference their just-inserted parent).
fn migrations_from(from: u32) -> Vec<&'static str> {
    let mut q: Vec<&'static str> = Vec::new();
    if from < 2 {
        // v2 — the accounts/transfer ledger; carries the Chapter-2 `expense` rows into `txn`.
        q.extend_from_slice(&[
            "CREATE TABLE IF NOT EXISTS account(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, \
             kind TEXT NOT NULL DEFAULT 'asset', opening TEXT NOT NULL DEFAULT '0', sort INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS txn(id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, \
             kind TEXT NOT NULL, amount TEXT NOT NULL, account_id INTEGER NOT NULL, to_account_id INTEGER, \
             category TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT '')",
            "INSERT INTO account(name, kind, opening, sort) SELECT 'Cash', 'asset', '0', 0 \
             WHERE NOT EXISTS (SELECT 1 FROM account)",
            "CREATE TABLE IF NOT EXISTS expense(id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, \
             amount TEXT NOT NULL, category TEXT NOT NULL, note TEXT NOT NULL DEFAULT '')",
            "INSERT INTO txn(ts, kind, amount, account_id, category, note) \
             SELECT ts, 'expense', amount, (SELECT MIN(id) FROM account), category, note FROM expense",
            "DROP TABLE expense",
            "PRAGMA user_version = 2",
        ]);
    }
    if from < 3 {
        // v3 — categories with subcategories (parent_id), plus a seeded default set.
        q.extend_from_slice(&[
            "CREATE TABLE IF NOT EXISTS category(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, \
             kind TEXT NOT NULL, parent_id INTEGER, sort INTEGER NOT NULL DEFAULT 0)",
            "INSERT INTO category(name, kind, sort) VALUES ('Food & Drink','expense',1)",
            "INSERT INTO category(name, kind, sort) VALUES ('Transport','expense',2)",
            "INSERT INTO category(name, kind, sort) VALUES ('Housing','expense',3)",
            "INSERT INTO category(name, kind, sort) VALUES ('Health','expense',4)",
            "INSERT INTO category(name, kind, sort) VALUES ('Shopping','expense',5)",
            "INSERT INTO category(name, kind, sort) VALUES ('Entertainment','expense',6)",
            "INSERT INTO category(name, kind, sort) VALUES ('Other','expense',99)",
            "INSERT INTO category(name, kind, parent_id, sort) \
             SELECT 'Groceries','expense',id,1 FROM category WHERE name='Food & Drink' AND kind='expense'",
            "INSERT INTO category(name, kind, parent_id, sort) \
             SELECT 'Restaurants','expense',id,2 FROM category WHERE name='Food & Drink' AND kind='expense'",
            "INSERT INTO category(name, kind, parent_id, sort) \
             SELECT 'Coffee','expense',id,3 FROM category WHERE name='Food & Drink' AND kind='expense'",
            "INSERT INTO category(name, kind, parent_id, sort) \
             SELECT 'Fuel','expense',id,1 FROM category WHERE name='Transport' AND kind='expense'",
            "INSERT INTO category(name, kind, parent_id, sort) \
             SELECT 'Transit','expense',id,2 FROM category WHERE name='Transport' AND kind='expense'",
            "INSERT INTO category(name, kind, sort) VALUES ('Salary','income',1)",
            "INSERT INTO category(name, kind, sort) VALUES ('Gifts','income',2)",
            "INSERT INTO category(name, kind, sort) VALUES ('Other','income',99)",
            "PRAGMA user_version = 3",
        ]);
    }
    if from < 4 {
        // v4 — recurring rules. `next_date` is the next occurrence to post; materialization advances it.
        q.extend_from_slice(&[
            "CREATE TABLE IF NOT EXISTS recurring(id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, \
             amount TEXT NOT NULL, account_id INTEGER NOT NULL, to_account_id INTEGER, \
             category TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT '', freq TEXT NOT NULL, \
             next_date TEXT NOT NULL, end_date TEXT)",
            "PRAGMA user_version = 4",
        ]);
    }
    if from < SCHEMA_VERSION {
        // v5 — a key/value settings store (UI language override + base currency, persisted across launches).
        // (The latest block gates on SCHEMA_VERSION; earlier blocks use their literal target version.)
        q.extend_from_slice(&[
            "CREATE TABLE IF NOT EXISTS setting(key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            "PRAGMA user_version = 5",
        ]);
    }
    q
}

const LOAD_ACCOUNTS: &str = "SELECT id, name, kind, opening, sort FROM account ORDER BY sort, id";
const LOAD_CATEGORIES: &str =
    "SELECT id, name, kind, parent_id, sort FROM category ORDER BY kind, sort, id";
const LOAD_TXNS: &str = "SELECT id, ts, kind, amount, account_id, to_account_id, category, note \
                         FROM txn ORDER BY ts DESC, id DESC";
const LOAD_RECURRING: &str =
    "SELECT id, kind, amount, account_id, to_account_id, category, note, freq, next_date, end_date \
     FROM recurring ORDER BY next_date, id";
const LOAD_SETTINGS: &str = "SELECT key, value FROM setting";

/// The currencies the picker offers, paired with their ISO code (used to persist + restore the choice).
const CURRENCIES: [(Currency, &str); 6] = [
    (Currency::Eur, "EUR"),
    (Currency::Usd, "USD"),
    (Currency::Gbp, "GBP"),
    (Currency::Chf, "CHF"),
    (Currency::Uah, "UAH"),
    (Currency::Rsd, "RSD"),
];

fn currency_code(c: Currency) -> &'static str {
    CURRENCIES.iter().find(|(cur, _)| *cur == c).map_or("EUR", |(_, code)| *code)
}

fn currency_from_code(code: &str) -> Option<Currency> {
    CURRENCIES.iter().find(|(_, c)| *c == code).map(|(cur, _)| *cur)
}

// ---- enums ----

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Screen {
    #[default]
    Bills,
    Stats,
    Assets,
    Settings,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Period {
    Day,
    #[default]
    Month,
    All,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum TxnKind {
    #[default]
    Expense,
    Income,
    Transfer,
}

impl TxnKind {
    fn db(self) -> &'static str {
        match self {
            TxnKind::Expense => "expense",
            TxnKind::Income => "income",
            TxnKind::Transfer => "transfer",
        }
    }
    fn from_db(s: &str) -> Self {
        match s {
            "income" => TxnKind::Income,
            "transfer" => TxnKind::Transfer,
            _ => TxnKind::Expense,
        }
    }
    /// The category kind a transaction of this kind draws from (transfers have no category).
    fn category_kind(self) -> &'static str {
        match self {
            TxnKind::Income => "income",
            _ => "expense",
        }
    }
}

/// How often a recurring rule fires.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Freq {
    #[default]
    Daily,
    Weekly,
    Monthly,
}

impl Freq {
    fn db(self) -> &'static str {
        match self {
            Freq::Daily => "daily",
            Freq::Weekly => "weekly",
            Freq::Monthly => "monthly",
        }
    }
    fn from_db(s: &str) -> Self {
        match s {
            "weekly" => Freq::Weekly,
            "monthly" => Freq::Monthly,
            _ => Freq::Daily,
        }
    }
    /// The catalog key for this frequency's label.
    fn key(self) -> &'static str {
        match self {
            Freq::Daily => "freq.daily",
            Freq::Weekly => "freq.weekly",
            Freq::Monthly => "freq.monthly",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum AccountKind {
    #[default]
    Asset,
    Liability,
}

impl AccountKind {
    fn db(self) -> &'static str {
        match self {
            AccountKind::Asset => "asset",
            AccountKind::Liability => "liability",
        }
    }
    fn from_db(s: &str) -> Self {
        if s == "liability" { AccountKind::Liability } else { AccountKind::Asset }
    }
}

// ---- rows ----

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    pub id: u32,
    pub name: String,
    pub kind: AccountKind,
    pub opening: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Category {
    pub id: u32,
    pub name: String,
    pub kind: String, // "income" | "expense"
    pub parent_id: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Txn {
    pub id: u32,
    pub ts: String,
    pub kind: TxnKind,
    pub amount: f64,
    pub account_id: u32,
    pub to_account_id: Option<u32>,
    pub category: String,
    pub note: String,
}

impl Txn {
    fn day(&self) -> &str {
        self.ts.get(..10).unwrap_or(&self.ts)
    }
}

/// A scheduled rule. Its `next_date` is the next occurrence still to post; materialization posts every
/// occurrence up to today and advances `next_date` past it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recurring {
    pub id: u32,
    pub kind: TxnKind,
    pub amount: f64,
    pub account_id: u32,
    pub to_account_id: Option<u32>,
    pub category: String,
    pub note: String,
    pub freq: Freq,
    pub next_date: String,        // "YYYY-MM-DD"
    pub end_date: Option<String>, // inclusive; None = no end
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // UI state — several independent on/off flags is natural here
pub struct Model {
    screen: Screen,
    period: Period,
    stats_kind: TxnKind,         // expense/income toggle on the Stats tab
    locale: Locale,              // formatting locale, from the device at startup
    currency: Currency,          // base currency — persisted setting, else defaulted from the device
    currency_pinned: bool,       // true once a saved/chosen currency wins over the device default
    device_lang: String,         // UI language negotiated from the device at startup
    lang_override: Option<String>, // a manual language choice from Settings (None = follow the device)
    today: String,
    accounts: Vec<Account>,
    categories: Vec<Category>,
    txns: Vec<Txn>,
    recurring: Vec<Recurring>,
    recurring_loaded: bool, // gate so materialization waits for both the rules and `today`
    lang_open: bool,        // Settings: the language "select" is expanded
    cur_open: bool,         // Settings: the currency "select" is expanded
    lock_enabled: bool,     // the app-lock setting (persisted)
    locked: bool,           // runtime: the app is currently locked, awaiting biometric unlock
    lock_checked: bool,     // we've done the one-time launch lock decision (don't re-lock on reloads)
    dark: bool,             // dark mode (persisted)
    form_error: Option<&'static str>, // a validation error (catalog key) to show in the open sheet
    pending_sql: Vec<String>,

    // "new transaction" sheet
    adding: bool,
    draft_kind: TxnKind,
    draft_amount: String,
    draft_account: Option<u32>,
    draft_to_account: Option<u32>,
    draft_category: String,      // the chosen category *name*
    draft_date: String,          // "YYYY-MM-DD" (defaults to today; changeable via the date picker)
    draft_note: String,
    draft_freq: Option<Freq>,    // the "Repeat" choice (None = a one-off transaction)

    // "new / edit account" sheet
    adding_account: bool,
    editing_account: Option<u32>, // Some(id) when editing an existing account, None when adding
    acc_name: String,
    acc_kind: AccountKind,
    acc_opening: String,

    // "manage categories" screen + its add/edit sheet
    managing_categories: bool,
    cat_sheet: bool,              // the add/edit category sheet is open
    editing_category: Option<u32>, // Some(id) when renaming, None when adding
    cat_kind: TxnKind,           // which set is shown/edited (income/expense)
    cat_name: String,            // the category's name (draft)
    cat_parent: Option<u32>,     // add under this top-level (None = a new top-level category)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Switch(Screen),
    SetPeriod(Period),
    SetStatsKind(TxnKind),
    GotLocale(String),
    ToggleLangPicker,
    ToggleCurPicker,
    SetLang(Option<String>),
    SetCurrency(Currency),
    SetLock(bool),
    SetDark(bool),
    Unlock,
    Authed(bool),
    SettingsLoaded(String),
    // schema / load
    Schema(String),
    Migrated,
    AccountsLoaded(String),
    CategoriesLoaded(String),
    Loaded(String),
    RecurringLoaded(String),
    GotToday(String),
    Reload,
    Posted, // a fire-and-forget materialization write completed (no-op)
    // new transaction
    StartAdd,
    CancelAdd,
    SetKind(TxnKind),
    SetAccount(u32),
    SetToAccount(u32),
    SetCategory(String),
    SetFreq(Option<Freq>),
    PickDate,
    DatePicked(String),
    Save,
    Saved(bool),
    Delete(u32),
    DeleteRecurring(u32),
    // new account
    StartAddAccount,
    EditAccount(u32),
    DeleteAccount(u32),
    CancelAddAccount,
    SetAccKind(AccountKind),
    SaveAccount,
    AccountSaved(bool),
    // manage categories
    StartManageCategories,
    CancelManageCategories,
    SetCatKind(TxnKind),
    StartAddCategory,
    EditCategory(u32),
    CancelCatSheet,
    SetCatParent(Option<u32>),
    SaveCategory,
    CategoryChanged(bool),
    DeleteCategory(u32),
    // data: export / backup / restore
    ExportCsv,
    Backup,
    DoExport(String, String), // (sandbox path, save-as name) — export after the write lands
    ExportDone(bool),
    Restore,
    RestorePicked(String), // a file:// URI from the picker
    RestoreRead(String),   // the file's contents
}

#[derive(Default)]
pub struct SaldoApp;

impl MobilerApp for SaldoApp {
    type Event = Msg;
    type Model = Model;

    fn init(&self, _model: &mut Model, cx: &mut Cx<Msg>) {
        cx.plugin("sqlite", "query", "PRAGMA user_version", |r| {
            Msg::Schema(if r.ok { r.output } else { "[]".to_string() })
        });
        cx.now(|r| Msg::GotToday(r.output));
        cx.device_locale(|r| Msg::GotLocale(r.output));
    }

    #[allow(clippy::too_many_lines)]
    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Switch(s) => model.screen = s,
            Msg::SetPeriod(p) => model.period = p,
            Msg::SetStatsKind(k) => model.stats_kind = k,
            Msg::GotLocale(tag) => {
                if let Some(loc) = Locale::from_tag(&tag) {
                    model.locale = loc;
                    // Only fill the currency from the device if the user hasn't pinned one.
                    if !model.currency_pinned {
                        model.currency = default_currency(loc);
                    }
                }
                model.device_lang = negotiate(&tag, &SUPPORTED, "en");
            }
            Msg::ToggleLangPicker => model.lang_open = !model.lang_open,
            Msg::ToggleCurPicker => model.cur_open = !model.cur_open,
            Msg::SetLang(choice) => {
                save_setting(cx, "lang", choice.as_deref().unwrap_or(""));
                model.lang_override = choice;
                model.lang_open = false; // collapse the "select" after choosing
            }
            Msg::SetCurrency(c) => {
                model.currency = c;
                model.currency_pinned = true;
                save_setting(cx, "currency", currency_code(c));
                model.cur_open = false;
            }
            Msg::SetLock(on) => {
                model.lock_enabled = on;
                save_setting(cx, "lock", if on { "1" } else { "" });
            }
            Msg::SetDark(on) => {
                model.dark = on;
                save_setting(cx, "dark", if on { "1" } else { "" });
            }
            Msg::Unlock => {
                cx.plugin("biometric", "authenticate", tr(model, "lock.prompt"), |r| Msg::Authed(r.ok));
            }
            Msg::Authed(ok) => model.locked = !ok,
            Msg::SettingsLoaded(json) => {
                // Lock the app only on the FIRST settings load after launch — not on every reload.
                let first_load = !model.lock_checked;
                model.lock_checked = true;
                for (k, v) in parse_settings(&json) {
                    match k.as_str() {
                        "lang" if !v.is_empty() => model.lang_override = Some(v),
                        "currency" => {
                            if let Some(c) = currency_from_code(&v) {
                                model.currency = c;
                                model.currency_pinned = true;
                            }
                        }
                        "lock" if v == "1" => {
                            model.lock_enabled = true;
                            if first_load {
                                model.locked = true;
                                cx.plugin("biometric", "authenticate", tr(model, "lock.prompt"), |r| {
                                    Msg::Authed(r.ok)
                                });
                            }
                        }
                        "dark" if v == "1" => model.dark = true,
                        _ => {}
                    }
                }
            }

            Msg::Schema(json) => {
                let v = pragma_int(&json);
                model.pending_sql = migrations_from(v).into_iter().map(String::from).collect();
                run_migration(model, cx);
            }
            Msg::Migrated => {
                if !model.pending_sql.is_empty() {
                    model.pending_sql.remove(0);
                }
                run_migration(model, cx);
            }
            Msg::Reload => load_all(cx),
            Msg::AccountsLoaded(json) => model.accounts = parse_accounts(&json),
            Msg::CategoriesLoaded(json) => model.categories = parse_categories(&json),
            Msg::Loaded(json) => model.txns = parse_txns(&json),
            Msg::RecurringLoaded(json) => {
                model.recurring = parse_recurring(&json);
                model.recurring_loaded = true;
                materialize(model, cx); // post any rules that came due (no-op once caught up)
            }
            Msg::Posted => {} // a materialization write landed; the reload after them refreshes the model
            Msg::GotToday(s) => {
                if let Some(d) = s.get(..10) {
                    model.today = d.to_string();
                }
                materialize(model, cx); // `today` may have been the missing half
            }

            Msg::StartAdd => {
                model.adding = true;
                model.form_error = None;
                model.draft_kind = TxnKind::Expense;
                model.draft_amount.clear();
                model.draft_category.clear();
                model.draft_note.clear();
                model.draft_date = model.today.clone();
                model.draft_account = model.accounts.first().map(|a| a.id);
                model.draft_to_account = model.accounts.get(1).map(|a| a.id);
                model.draft_freq = None;
            }
            Msg::CancelAdd => {
                model.adding = false;
                model.form_error = None;
            }
            Msg::SetKind(k) => {
                model.draft_kind = k;
                model.draft_category.clear(); // categories differ per kind
            }
            Msg::SetAccount(id) => {
                model.draft_account = Some(id);
                model.form_error = None;
            }
            Msg::SetToAccount(id) => {
                model.draft_to_account = Some(id);
                model.form_error = None;
            }
            Msg::SetCategory(name) => {
                model.draft_category = name;
                model.form_error = None;
            }
            Msg::SetFreq(f) => model.draft_freq = f,
            Msg::PickDate => cx.pick_date(|r| Msg::DatePicked(if r.ok { r.output } else { String::new() })),
            Msg::DatePicked(date) => {
                if !date.is_empty() {
                    model.draft_date = date;
                }
            }
            Msg::Save => {
                if let Err(why) = validate_txn(model) {
                    model.form_error = Some(why); // shown inline in the sheet
                    return;
                }
                let Some(amount) = parse_amount(&model.draft_amount) else { return };
                let Some(account_id) = model.draft_account else { return };
                // Entries are date-precision; ordering within a day falls back to id.
                let ts = format!("{} 12:00:00", model.draft_date);
                let to = if model.draft_kind == TxnKind::Transfer {
                    model.draft_to_account.map_or(serde_json::Value::Null, |id| serde_json::Value::from(id.to_string()))
                } else {
                    serde_json::Value::Null
                };
                let category = if model.draft_kind == TxnKind::Transfer {
                    String::new()
                } else {
                    model.draft_category.clone()
                };
                let sql = if let Some(freq) = model.draft_freq {
                    // A scheduled rule; materialization posts the first (and future) occurrences.
                    serde_json::json!({
                        "sql": "INSERT INTO recurring(kind, amount, account_id, to_account_id, category, \
                                note, freq, next_date, end_date) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
                        "args": [model.draft_kind.db(), amount.to_string(), account_id.to_string(),
                                 to, category, model.draft_note.trim(), freq.db(), model.draft_date],
                    })
                } else {
                    serde_json::json!({
                        "sql": "INSERT INTO txn(ts, kind, amount, account_id, to_account_id, category, note) \
                                VALUES (?, ?, ?, ?, ?, ?, ?)",
                        "args": [ts, model.draft_kind.db(), amount.to_string(),
                                 account_id.to_string(), to, category, model.draft_note.trim()],
                    })
                }
                .to_string();
                cx.plugin("sqlite", "exec", sql, |r| Msg::Saved(r.ok));
            }
            Msg::Saved(ok) => {
                if ok {
                    model.adding = false;
                    model.form_error = None;
                    load_all(cx);
                } else {
                    model.form_error = Some("err.save");
                }
            }
            Msg::Delete(id) => {
                let sql = serde_json::json!({ "sql": "DELETE FROM txn WHERE id = ?", "args": [id.to_string()] })
                    .to_string();
                cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
            }
            Msg::DeleteRecurring(id) => {
                // Deletes the rule only; transactions it already posted stay in the ledger.
                let sql =
                    serde_json::json!({ "sql": "DELETE FROM recurring WHERE id = ?", "args": [id.to_string()] })
                        .to_string();
                cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
            }

            Msg::StartAddAccount => {
                model.adding_account = true;
                model.editing_account = None;
                model.form_error = None;
                model.acc_name.clear();
                model.acc_kind = AccountKind::Asset;
                model.acc_opening.clear();
            }
            Msg::EditAccount(id) => {
                if let Some(a) = model.accounts.iter().find(|a| a.id == id) {
                    model.adding_account = true;
                    model.editing_account = Some(id);
                    model.form_error = None;
                    model.acc_name = a.name.clone();
                    model.acc_kind = a.kind;
                    model.acc_opening = a.opening.to_string();
                }
            }
            Msg::DeleteAccount(id) => {
                // Keep the ledger consistent: refuse to delete an account that still has transactions.
                let in_use = model
                    .txns
                    .iter()
                    .any(|t| t.account_id == id || t.to_account_id == Some(id));
                if in_use {
                    cx.notify("toast", "show", tr(model, "err.acct_in_use"));
                } else {
                    let sql = serde_json::json!({ "sql": "DELETE FROM account WHERE id = ?", "args": [id.to_string()] })
                        .to_string();
                    cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
                }
            }
            Msg::CancelAddAccount => {
                model.adding_account = false;
                model.form_error = None;
            }
            Msg::SetAccKind(k) => model.acc_kind = k,
            Msg::SaveAccount => {
                if model.acc_name.trim().is_empty() {
                    model.form_error = Some("err.accname");
                    return;
                }
                let opening = model.acc_opening.trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
                let sql = if let Some(id) = model.editing_account {
                    serde_json::json!({
                        "sql": "UPDATE account SET name = ?, kind = ?, opening = ? WHERE id = ?",
                        "args": [model.acc_name.trim(), model.acc_kind.db(), opening.to_string(), id.to_string()],
                    })
                } else {
                    serde_json::json!({
                        "sql": "INSERT INTO account(name, kind, opening, sort) VALUES (?, ?, ?, 0)",
                        "args": [model.acc_name.trim(), model.acc_kind.db(), opening.to_string()],
                    })
                }
                .to_string();
                cx.plugin("sqlite", "exec", sql, |r| Msg::AccountSaved(r.ok));
            }
            Msg::AccountSaved(ok) => {
                if ok {
                    model.adding_account = false;
                    model.form_error = None;
                    load_all(cx);
                } else {
                    model.form_error = Some("err.save");
                }
            }

            Msg::StartManageCategories => {
                model.managing_categories = true;
                model.cat_kind = TxnKind::Expense;
            }
            Msg::CancelManageCategories => model.managing_categories = false,
            Msg::SetCatKind(k) => model.cat_kind = k,
            Msg::StartAddCategory => {
                model.cat_sheet = true;
                model.editing_category = None;
                model.form_error = None;
                model.cat_name.clear();
                model.cat_parent = None;
            }
            Msg::EditCategory(id) => {
                if let Some(c) = model.categories.iter().find(|c| c.id == id) {
                    model.cat_sheet = true;
                    model.editing_category = Some(id);
                    model.form_error = None;
                    model.cat_name = c.name.clone();
                    model.cat_kind = TxnKind::from_db(&c.kind);
                    model.cat_parent = c.parent_id;
                }
            }
            Msg::CancelCatSheet => {
                model.cat_sheet = false;
                model.form_error = None;
            }
            Msg::SetCatParent(p) => model.cat_parent = p,
            Msg::SaveCategory => {
                if model.cat_name.trim().is_empty() {
                    model.form_error = Some("err.cat_name");
                    return;
                }
                let sql = if let Some(id) = model.editing_category {
                    serde_json::json!({
                        "sql": "UPDATE category SET name = ? WHERE id = ?",
                        "args": [model.cat_name.trim(), id.to_string()],
                    })
                } else {
                    let parent =
                        model.cat_parent.map_or(serde_json::Value::Null, |id| id.to_string().into());
                    serde_json::json!({
                        "sql": "INSERT INTO category(name, kind, parent_id, sort) VALUES (?, ?, ?, 99)",
                        "args": [model.cat_name.trim(), model.cat_kind.category_kind(), parent],
                    })
                }
                .to_string();
                cx.plugin("sqlite", "exec", sql, |r| Msg::CategoryChanged(r.ok));
            }
            Msg::CategoryChanged(ok) => {
                if ok {
                    model.cat_sheet = false;
                    model.cat_name.clear();
                    model.form_error = None;
                    load_all(cx);
                } else {
                    model.form_error = Some("err.save");
                }
            }
            Msg::DeleteCategory(id) => {
                // Remove the category and any subcategories. Past transactions keep their category *name*,
                // so the ledger is untouched — the category just leaves the picker.
                let sql = serde_json::json!({
                    "sql": "DELETE FROM category WHERE id = ? OR parent_id = ?",
                    "args": [id.to_string(), id.to_string()],
                })
                .to_string();
                cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
            }

            // Export the ledger as CSV: build the text in the core, write it to the sandbox, then hand
            // it to the system save-picker. The two-step (write → export) is why DoExport carries the
            // name through the write's callback.
            Msg::ExportCsv => {
                let name = format!("Saldo-{}.csv", model.today);
                write_and_export(cx, "saldo-export.csv", name, &build_csv(model));
            }
            Msg::Backup => {
                let name = format!("Saldo-Backup-{}.json", model.today);
                write_and_export(cx, "saldo-backup.json", name, &build_backup(model));
            }
            Msg::DoExport(path, name) => {
                let sql = serde_json::json!({ "path": path, "name": name }).to_string();
                cx.plugin("files", "export", sql, |r| Msg::ExportDone(r.ok));
            }
            Msg::ExportDone(ok) => {
                cx.notify("toast", "show", tr(model, if ok { "data.saved" } else { "data.cancelled" }));
            }
            // Restore: pick a backup file → read it → replace every table from it → reload.
            Msg::Restore => cx.plugin("filepicker", "pick", "", |r| {
                if r.ok { Msg::RestorePicked(r.output) } else { Msg::ExportDone(false) }
            }),
            Msg::RestorePicked(uri) => {
                let sql = serde_json::json!({ "path": uri }).to_string();
                cx.plugin("files", "read", sql, |r| {
                    if r.ok { Msg::RestoreRead(r.output) } else { Msg::RestoreRead(String::new()) }
                });
            }
            Msg::RestoreRead(json) => match serde_json::from_str::<Backup>(&json) {
                Ok(backup) => {
                    for stmt in restore_statements(&backup) {
                        cx.plugin("sqlite", "exec", stmt, |_| Msg::Posted);
                    }
                    cx.notify("toast", "show", tr(model, "data.restored"));
                    load_all(cx);
                }
                Err(_) => cx.notify("toast", "show", tr(model, "data.bad_backup")),
            },
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        if let InputValue::Text(v) = value {
            model.form_error = None; // typing clears a shown validation error
            match id {
                "amount" => model.draft_amount = v,
                "note" => model.draft_note = v,
                "acc_name" => model.acc_name = v,
                "acc_opening" => model.acc_opening = v,
                "cat_name" => model.cat_name = v,
                _ => {}
            }
        }
    }

    fn view(&self, model: &Model) -> Widget {
        if model.locked {
            // Cover everything with a lock screen until biometric (or the device passcode) succeeds.
            let lock = column(vec![
                spacer(Spacing::Xl),
                spacer(Spacing::Xl),
                subtitle(tr(model, "lock.title")),
                caption(tr(model, "lock.hint")),
                spacer(Spacing::Md),
                button(tr(model, "lock.unlock"), ButtonStyle::Filled, Msg::Unlock),
            ]);
            return with_theme(scaffold("Saldo", model.dark, vec![], lock), saldo_theme());
        }
        let tabs = vec![
            tab(model.screen, Screen::Bills, tr(model, "tab.bills"), Icon::Home),
            tab(model.screen, Screen::Stats, tr(model, "tab.stats"), Icon::Star),
            tab(model.screen, Screen::Assets, tr(model, "tab.assets"), Icon::Cart),
            tab(model.screen, Screen::Settings, tr(model, "tab.settings"), Icon::Settings),
        ];
        // The Bills screen keeps the brand as its heading; the others use the (translated) tab name.
        // Category management is a full screen (not a modal sheet) so it always has a clear way out.
        let (heading, body) = match model.screen {
            Screen::Bills => ("Saldo".to_string(), bills(model)),
            Screen::Stats => (tr(model, "tab.stats"), stats(model)),
            Screen::Assets => (tr(model, "tab.assets"), assets(model)),
            Screen::Settings if model.managing_categories => {
                (tr(model, "categories.manage"), category_manager(model))
            }
            Screen::Settings => (tr(model, "tab.settings"), settings(model)),
        };

        let mut root = scaffold(heading, model.dark, tabs, body);
        match model.screen {
            Screen::Bills => root = with_fab(root, Icon::Add, Msg::StartAdd),
            Screen::Assets => root = with_fab(root, Icon::Add, Msg::StartAddAccount),
            _ => {}
        }
        if model.adding {
            root = with_sheet(root, tr(model, "sheet.newtxn"), txn_sheet(model), Msg::CancelAdd);
        } else if model.adding_account {
            let title =
                tr(model, if model.editing_account.is_some() { "sheet.editaccount" } else { "sheet.newaccount" });
            root = with_sheet(root, title, account_sheet(model), Msg::CancelAddAccount);
        } else if model.cat_sheet {
            let title =
                tr(model, if model.editing_category.is_some() { "sheet.editcategory" } else { "sheet.newcategory" });
            root = with_sheet(root, title, category_sheet(model), Msg::CancelCatSheet);
        }
        with_theme(root, saldo_theme())
    }
}

/// Saldo's brand: a confident teal (money/balance), friendly large corners, the system font.
fn saldo_theme() -> Theme {
    Theme {
        seed: Rgb::new(0x0E, 0x9F, 0x8E),
        accent: None,
        corner: Corner::Large,
        density: Density::Comfortable,
        font: FontFamily::System,
    }
}

// ---- effects helpers ----

fn run_migration(model: &mut Model, cx: &mut Cx<Msg>) {
    match model.pending_sql.first() {
        Some(sql) => {
            let sql = sql.clone();
            cx.plugin("sqlite", "exec", sql, |_| Msg::Migrated);
        }
        None => load_all(cx),
    }
}

fn load_all(cx: &mut Cx<Msg>) {
    cx.plugin("sqlite", "query", LOAD_ACCOUNTS, |r| {
        Msg::AccountsLoaded(if r.ok { r.output } else { "[]".to_string() })
    });
    cx.plugin("sqlite", "query", LOAD_CATEGORIES, |r| {
        Msg::CategoriesLoaded(if r.ok { r.output } else { "[]".to_string() })
    });
    cx.plugin("sqlite", "query", LOAD_TXNS, |r| {
        Msg::Loaded(if r.ok { r.output } else { "[]".to_string() })
    });
    cx.plugin("sqlite", "query", LOAD_RECURRING, |r| {
        Msg::RecurringLoaded(if r.ok { r.output } else { "[]".to_string() })
    });
    cx.plugin("sqlite", "query", LOAD_SETTINGS, |r| {
        Msg::SettingsLoaded(if r.ok { r.output } else { "[]".to_string() })
    });
}

/// Persist a single key/value setting (UPSERT). All sqlite args are strings.
fn save_setting(cx: &mut Cx<Msg>, key: &str, value: &str) {
    let sql = serde_json::json!({
        "sql": "INSERT INTO setting(key, value) VALUES (?, ?) \
                ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        "args": [key, value],
    })
    .to_string();
    cx.plugin("sqlite", "exec", sql, |_| Msg::Posted);
}

fn parse_settings(json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            Some((r.get("key")?.as_str()?.to_string(), r.get("value")?.as_str()?.to_string()))
        })
        .collect()
}

/// Post every recurring occurrence due on or before `today`, then reload. A no-op until both the rules
/// and `today` are loaded, and idempotent afterwards (once advanced, `next_date` is in the future).
/// Runs the writes in order before the reload — the sqlite plugin serializes on one connection.
fn materialize(model: &Model, cx: &mut Cx<Msg>) {
    if !model.recurring_loaded || model.today.is_empty() {
        return;
    }
    let (posts, updates) = due_posts(&model.recurring, &model.today);
    if posts.is_empty() {
        return;
    }
    for p in posts {
        let to = p.to_account_id.map_or(serde_json::Value::Null, |id| id.to_string().into());
        let sql = serde_json::json!({
            "sql": "INSERT INTO txn(ts, kind, amount, account_id, to_account_id, category, note) \
                    VALUES (?, ?, ?, ?, ?, ?, ?)",
            "args": [format!("{} 12:00:00", p.date), p.kind.db(), p.amount.to_string(),
                     p.account_id.to_string(), to, p.category, p.note],
        })
        .to_string();
        cx.plugin("sqlite", "exec", sql, |_| Msg::Posted);
    }
    for (id, next) in updates {
        let sql = serde_json::json!({
            "sql": "UPDATE recurring SET next_date = ? WHERE id = ?",
            "args": [next, id.to_string()],
        })
        .to_string();
        cx.plugin("sqlite", "exec", sql, |_| Msg::Posted);
    }
    load_all(cx); // reflect the new txns + advanced next_dates (re-fires materialize, now a no-op)
}

// ---- data: export / backup / restore (the `files` + `filepicker` plugins) ----

/// Write `content` to the app sandbox, then (on success) hand it to the system save-picker as `name`.
fn write_and_export(cx: &mut Cx<Msg>, path: &str, name: String, content: &str) {
    let p = path.to_string();
    let input = serde_json::json!({ "path": path, "content": content }).to_string();
    cx.plugin("files", "write", input, move |r| {
        if r.ok { Msg::DoExport(p, name) } else { Msg::ExportDone(false) }
    });
}

/// Quote a CSV field if it contains a comma, quote, or newline (doubling any embedded quotes).
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The whole ledger as CSV (one header row + a row per transaction), newest first as loaded.
fn build_csv(model: &Model) -> String {
    let mut out = String::from("date,type,amount,account,to_account,category,note\n");
    for t in &model.txns {
        let cols = [
            t.day().to_string(),
            t.kind.db().to_string(),
            format!("{:.2}", t.amount),
            acct_name(&model.accounts, t.account_id).to_string(),
            t.to_account_id.map(|id| acct_name(&model.accounts, id).to_string()).unwrap_or_default(),
            t.category.clone(),
            t.note.clone(),
        ];
        out.push_str(&cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// A full database snapshot — the JSON backup format (every table, ids included for exact restore).
#[derive(Serialize, Deserialize)]
struct Backup {
    version: u32,
    accounts: Vec<Account>,
    categories: Vec<Category>,
    txns: Vec<Txn>,
    recurring: Vec<Recurring>,
}

fn build_backup(model: &Model) -> String {
    serde_json::to_string(&Backup {
        version: SCHEMA_VERSION,
        accounts: model.accounts.clone(),
        categories: model.categories.clone(),
        txns: model.txns.clone(),
        recurring: model.recurring.clone(),
    })
    .unwrap_or_default()
}

/// SQL to replace every table from a backup: clear all four, then re-insert each row **with its id**
/// (so `account_id` / `parent_id` references survive). Returned as ready-to-exec `{sql, args}` strings.
fn restore_statements(b: &Backup) -> Vec<String> {
    let mut out = Vec::new();
    for table in ["txn", "recurring", "category", "account"] {
        out.push(serde_json::json!({ "sql": format!("DELETE FROM {table}") }).to_string());
    }
    for a in &b.accounts {
        out.push(serde_json::json!({
            "sql": "INSERT INTO account(id, name, kind, opening, sort) VALUES (?, ?, ?, ?, 0)",
            "args": [a.id.to_string(), a.name.as_str(), a.kind.db(), a.opening.to_string()],
        }).to_string());
    }
    for c in &b.categories {
        let parent = c.parent_id.map_or(serde_json::Value::Null, |p| p.to_string().into());
        out.push(serde_json::json!({
            "sql": "INSERT INTO category(id, name, kind, parent_id, sort) VALUES (?, ?, ?, ?, 0)",
            "args": [c.id.to_string(), c.name.as_str(), c.kind.as_str(), parent],
        }).to_string());
    }
    for t in &b.txns {
        let to = t.to_account_id.map_or(serde_json::Value::Null, |id| id.to_string().into());
        out.push(serde_json::json!({
            "sql": "INSERT INTO txn(id, ts, kind, amount, account_id, to_account_id, category, note) \
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            "args": [t.id.to_string(), t.ts.as_str(), t.kind.db(), t.amount.to_string(),
                     t.account_id.to_string(), to, t.category.as_str(), t.note.as_str()],
        }).to_string());
    }
    for r in &b.recurring {
        let to = r.to_account_id.map_or(serde_json::Value::Null, |id| id.to_string().into());
        let end = r.end_date.clone().map_or(serde_json::Value::Null, Into::into);
        out.push(serde_json::json!({
            "sql": "INSERT INTO recurring(id, kind, amount, account_id, to_account_id, category, note, \
                    freq, next_date, end_date) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            "args": [r.id.to_string(), r.kind.db(), r.amount.to_string(), r.account_id.to_string(),
                     to, r.category.as_str(), r.note.as_str(), r.freq.db(), r.next_date.as_str(), end],
        }).to_string());
    }
    out
}

fn tab(current: Screen, screen: Screen, label: impl Into<String>, icon: Icon) -> mobiler_core::Tab {
    mobiler_core::tab_icon(label, icon, current == screen, Msg::Switch(screen))
}

// ---- localization ----

/// The effective UI language: the manual override if set, otherwise the device-negotiated language.
fn lang(model: &Model) -> &str {
    model.lang_override.as_deref().unwrap_or(&model.device_lang)
}

/// Translate `key` into the model's effective language (see [`Catalog::tr`] for the fallback chain).
fn tr(model: &Model, key: &'static str) -> String {
    catalog().tr(key, lang(model)).to_string()
}

/// The app's translation table, built once. English is the base; a missing translation falls back to
/// English, then to the key. User data (account and category names) is *not* translated — only chrome.
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        Catalog::new("en")
            // tabs / headings
            .with("tab.bills", &[("en", "Bills"), ("de", "Buchungen"), ("fr", "Opérations"), ("it", "Movimenti"), ("uk", "Операції")])
            .with("tab.stats", &[("en", "Stats"), ("de", "Statistik"), ("fr", "Stats"), ("it", "Statistiche"), ("uk", "Статистика")])
            .with("tab.assets", &[("en", "Assets"), ("de", "Konten"), ("fr", "Comptes"), ("it", "Conti"), ("uk", "Рахунки")])
            .with("tab.settings", &[("en", "Settings"), ("de", "Einstellungen"), ("fr", "Réglages"), ("it", "Impostazioni"), ("uk", "Налаштування")])
            // periods
            .with("period.day", &[("en", "Day"), ("de", "Tag"), ("fr", "Jour"), ("it", "Giorno"), ("uk", "День")])
            .with("period.month", &[("en", "Month"), ("de", "Monat"), ("fr", "Mois"), ("it", "Mese"), ("uk", "Місяць")])
            .with("period.all", &[("en", "All"), ("de", "Alle"), ("fr", "Tout"), ("it", "Tutto"), ("uk", "Усе")])
            // money / ledger
            .with("income", &[("en", "Income"), ("de", "Einnahmen"), ("fr", "Revenus"), ("it", "Entrate"), ("uk", "Дохід")])
            .with("expense", &[("en", "Expense"), ("de", "Ausgaben"), ("fr", "Dépenses"), ("it", "Uscite"), ("uk", "Витрати")])
            .with("net", &[("en", "Net"), ("de", "Saldo"), ("fr", "Solde"), ("it", "Saldo"), ("uk", "Баланс")])
            .with("delete", &[("en", "Delete"), ("de", "Löschen"), ("fr", "Supprimer"), ("it", "Elimina"), ("uk", "Видалити")])
            .with("bills.empty", &[("en", "Nothing here yet — tap + to add a transaction."), ("de", "Noch nichts da — tippe auf +, um eine Buchung hinzuzufügen."), ("fr", "Rien pour l'instant — touchez + pour ajouter une opération."), ("it", "Ancora niente — tocca + per aggiungere un movimento."), ("uk", "Поки що порожньо — натисніть +, щоб додати запис.")])
            // assets
            .with("assets.assets", &[("en", "Assets"), ("de", "Vermögen"), ("fr", "Actifs"), ("it", "Attività"), ("uk", "Активи")])
            .with("assets.liabilities", &[("en", "Liabilities"), ("de", "Schulden"), ("fr", "Passifs"), ("it", "Passività"), ("uk", "Пасиви")])
            .with("networth", &[("en", "Net worth"), ("de", "Nettovermögen"), ("fr", "Valeur nette"), ("it", "Patrimonio netto"), ("uk", "Чисті активи")])
            .with("assets.empty", &[("en", "Tap + to add an account."), ("de", "Tippe auf +, um ein Konto hinzuzufügen."), ("fr", "Touchez + pour ajouter un compte."), ("it", "Tocca + per aggiungere un conto."), ("uk", "Натисніть +, щоб додати рахунок.")])
            .with("liability.tag", &[("en", " (liability)"), ("de", " (Schuld)"), ("fr", " (passif)"), ("it", " (passività)"), ("uk", " (пасив)")])
            // stats
            .with("stats.bycategory", &[("en", "By category"), ("de", "Nach Kategorie"), ("fr", "Par catégorie"), ("it", "Per categoria"), ("uk", "За категоріями")])
            .with("stats.no_expense", &[("en", "No expenses in this period."), ("de", "Keine Ausgaben in diesem Zeitraum."), ("fr", "Aucune dépense sur cette période."), ("it", "Nessuna uscita in questo periodo."), ("uk", "Немає витрат за цей період.")])
            .with("stats.no_income", &[("en", "No income in this period."), ("de", "Keine Einnahmen in diesem Zeitraum."), ("fr", "Aucun revenu sur cette période."), ("it", "Nessuna entrata in questo periodo."), ("uk", "Немає доходів за цей період.")])
            .with("stats.empty", &[("en", "Add a few transactions and your charts appear here."), ("de", "Füge ein paar Buchungen hinzu, dann erscheinen hier deine Diagramme."), ("fr", "Ajoutez quelques opérations et vos graphiques apparaîtront ici."), ("it", "Aggiungi qualche movimento e i grafici appariranno qui."), ("uk", "Додайте кілька записів — і тут з'являться діаграми.")])
            .with("stats.trend", &[("en", "Net-worth trend"), ("de", "Vermögensverlauf"), ("fr", "Évolution du patrimoine"), ("it", "Andamento del patrimonio"), ("uk", "Динаміка капіталу")])
            .with("stats.monthly", &[("en", "Monthly income vs expense"), ("de", "Einnahmen und Ausgaben pro Monat"), ("fr", "Revenus et dépenses par mois"), ("it", "Entrate e uscite mensili"), ("uk", "Доходи та витрати за місяць")])
            // entry sheet
            .with("sheet.newtxn", &[("en", "New transaction"), ("de", "Neue Buchung"), ("fr", "Nouvelle opération"), ("it", "Nuovo movimento"), ("uk", "Новий запис")])
            .with("type.transfer", &[("en", "Transfer"), ("de", "Umbuchung"), ("fr", "Virement"), ("it", "Trasferimento"), ("uk", "Переказ")])
            .with("field.amount", &[("en", "Amount (e.g. 12.50)"), ("de", "Betrag (z. B. 12.50)"), ("fr", "Montant (p. ex. 12.50)"), ("it", "Importo (es. 12.50)"), ("uk", "Сума (напр. 12.50)")])
            .with("field.date", &[("en", "Date"), ("de", "Datum"), ("fr", "Date"), ("it", "Data"), ("uk", "Дата")])
            .with("action.change", &[("en", "Change"), ("de", "Ändern"), ("fr", "Modifier"), ("it", "Modifica"), ("uk", "Змінити")])
            .with("field.account", &[("en", "Account"), ("de", "Konto"), ("fr", "Compte"), ("it", "Conto"), ("uk", "Рахунок")])
            .with("field.from", &[("en", "From account"), ("de", "Von Konto"), ("fr", "Compte source"), ("it", "Dal conto"), ("uk", "З рахунку")])
            .with("field.to", &[("en", "To account"), ("de", "Auf Konto"), ("fr", "Compte destinataire"), ("it", "Al conto"), ("uk", "На рахунок")])
            .with("field.category", &[("en", "Category"), ("de", "Kategorie"), ("fr", "Catégorie"), ("it", "Categoria"), ("uk", "Категорія")])
            .with("field.note", &[("en", "Note (optional)"), ("de", "Notiz (optional)"), ("fr", "Note (facultatif)"), ("it", "Nota (facoltativa)"), ("uk", "Нотатка (необов'язково)")])
            .with("action.save", &[("en", "Save"), ("de", "Speichern"), ("fr", "Enregistrer"), ("it", "Salva"), ("uk", "Зберегти")])
            .with("action.cancel", &[("en", "Cancel"), ("de", "Abbrechen"), ("fr", "Annuler"), ("it", "Annulla"), ("uk", "Скасувати")])
            // recurring
            .with("field.repeat", &[("en", "Repeat"), ("de", "Wiederholen"), ("fr", "Répéter"), ("it", "Ripeti"), ("uk", "Повторювати")])
            .with("freq.once", &[("en", "Once"), ("de", "Einmal"), ("fr", "Une fois"), ("it", "Una volta"), ("uk", "Один раз")])
            .with("freq.daily", &[("en", "Daily"), ("de", "Täglich"), ("fr", "Quotidien"), ("it", "Giornaliero"), ("uk", "Щодня")])
            .with("freq.weekly", &[("en", "Weekly"), ("de", "Wöchentlich"), ("fr", "Hebdomadaire"), ("it", "Settimanale"), ("uk", "Щотижня")])
            .with("freq.monthly", &[("en", "Monthly"), ("de", "Monatlich"), ("fr", "Mensuel"), ("it", "Mensile"), ("uk", "Щомісяця")])
            .with("settings.scheduled", &[("en", "Scheduled"), ("de", "Geplant"), ("fr", "Planifié"), ("it", "Pianificati"), ("uk", "Заплановані")])
            .with("recurring.next", &[("en", "next"), ("de", "nächste"), ("fr", "prochain"), ("it", "prossimo"), ("uk", "наступний")])
            .with("recurring.empty", &[("en", "No scheduled transactions. Pick a Repeat in the entry sheet to add one."), ("de", "Keine geplanten Buchungen. Wähle im Eingabefenster eine Wiederholung."), ("fr", "Aucune opération planifiée. Choisissez une répétition dans la feuille de saisie."), ("it", "Nessun movimento pianificato. Scegli una ripetizione nella scheda di inserimento."), ("uk", "Немає запланованих записів. Виберіть повторення у формі додавання.")])
            // data (export / backup / restore)
            .with("settings.data", &[("en", "Data"), ("de", "Daten"), ("fr", "Données"), ("it", "Dati"), ("uk", "Дані")])
            .with("data.export_csv", &[("en", "Export CSV"), ("de", "CSV exportieren"), ("fr", "Exporter en CSV"), ("it", "Esporta CSV"), ("uk", "Експорт CSV")])
            .with("data.backup", &[("en", "Back up"), ("de", "Sichern"), ("fr", "Sauvegarder"), ("it", "Backup"), ("uk", "Резервна копія")])
            .with("data.restore", &[("en", "Restore from backup"), ("de", "Aus Sicherung wiederherstellen"), ("fr", "Restaurer depuis une sauvegarde"), ("it", "Ripristina da backup"), ("uk", "Відновити з резервної копії")])
            .with("data.hint", &[("en", "Export sends a CSV of your transactions to Files. A backup saves everything as JSON; restoring it replaces all current data."), ("de", "Der Export schickt eine CSV deiner Buchungen an Dateien. Eine Sicherung speichert alles als JSON; beim Wiederherstellen werden alle aktuellen Daten ersetzt."), ("fr", "L'export envoie un CSV de vos opérations vers Fichiers. Une sauvegarde enregistre tout en JSON ; la restauration remplace toutes les données actuelles."), ("it", "L'esportazione invia un CSV dei tuoi movimenti a File. Un backup salva tutto come JSON; il ripristino sostituisce tutti i dati attuali."), ("uk", "Експорт надсилає CSV ваших записів у «Файли». Резервна копія зберігає все як JSON; відновлення замінює всі поточні дані.")])
            .with("data.saved", &[("en", "Saved to Files."), ("de", "In Dateien gespeichert."), ("fr", "Enregistré dans Fichiers."), ("it", "Salvato in File."), ("uk", "Збережено у «Файлах».")])
            .with("data.cancelled", &[("en", "Cancelled."), ("de", "Abgebrochen."), ("fr", "Annulé."), ("it", "Annullato."), ("uk", "Скасовано.")])
            .with("data.restored", &[("en", "Restored from backup."), ("de", "Aus Sicherung wiederhergestellt."), ("fr", "Restauré depuis la sauvegarde."), ("it", "Ripristinato dal backup."), ("uk", "Відновлено з резервної копії.")])
            .with("data.bad_backup", &[("en", "Couldn't read that backup file."), ("de", "Diese Sicherungsdatei konnte nicht gelesen werden."), ("fr", "Impossible de lire ce fichier de sauvegarde."), ("it", "Impossibile leggere quel file di backup."), ("uk", "Не вдалося прочитати цей файл резервної копії.")])
            // account sheet
            .with("sheet.newaccount", &[("en", "New account"), ("de", "Neues Konto"), ("fr", "Nouveau compte"), ("it", "Nuovo conto"), ("uk", "Новий рахунок")])
            .with("sheet.editaccount", &[("en", "Edit account"), ("de", "Konto bearbeiten"), ("fr", "Modifier le compte"), ("it", "Modifica conto"), ("uk", "Редагувати рахунок")])
            .with("err.acct_in_use", &[("en", "Can't delete an account with transactions."), ("de", "Konto mit Buchungen kann nicht gelöscht werden."), ("fr", "Impossible de supprimer un compte avec des opérations."), ("it", "Impossibile eliminare un conto con movimenti."), ("uk", "Не можна видалити рахунок із записами.")])
            .with("field.accname", &[("en", "Account name (e.g. Cash, Bank)"), ("de", "Kontoname (z. B. Bargeld, Bank)"), ("fr", "Nom du compte (p. ex. Espèces, Banque)"), ("it", "Nome del conto (es. Contanti, Banca)"), ("uk", "Назва рахунку (напр. Готівка, Банк)")])
            .with("kind.asset", &[("en", "Asset"), ("de", "Aktiv"), ("fr", "Actif"), ("it", "Attivo"), ("uk", "Актив")])
            .with("kind.liability", &[("en", "Liability"), ("de", "Passiv"), ("fr", "Passif"), ("it", "Passivo"), ("uk", "Пасив")])
            .with("field.opening", &[("en", "Opening balance (optional)"), ("de", "Anfangssaldo (optional)"), ("fr", "Solde initial (facultatif)"), ("it", "Saldo iniziale (facoltativo)"), ("uk", "Початковий баланс (необов'язково)")])
            // settings
            .with("settings.language", &[("en", "Language"), ("de", "Sprache"), ("fr", "Langue"), ("it", "Lingua"), ("uk", "Мова")])
            .with("settings.currency", &[("en", "Currency"), ("de", "Währung"), ("fr", "Devise"), ("it", "Valuta"), ("uk", "Валюта")])
            // categories
            .with("settings.categories", &[("en", "Categories"), ("de", "Kategorien"), ("fr", "Catégories"), ("it", "Categorie"), ("uk", "Категорії")])
            .with("categories.manage", &[("en", "Manage categories"), ("de", "Kategorien verwalten"), ("fr", "Gérer les catégories"), ("it", "Gestisci categorie"), ("uk", "Керувати категоріями")])
            .with("categories.add", &[("en", "Add category"), ("de", "Kategorie hinzufügen"), ("fr", "Ajouter une catégorie"), ("it", "Aggiungi categoria"), ("uk", "Додати категорію")])
            .with("sheet.newcategory", &[("en", "New category"), ("de", "Neue Kategorie"), ("fr", "Nouvelle catégorie"), ("it", "Nuova categoria"), ("uk", "Нова категорія")])
            .with("sheet.editcategory", &[("en", "Rename category"), ("de", "Kategorie umbenennen"), ("fr", "Renommer la catégorie"), ("it", "Rinomina categoria"), ("uk", "Перейменувати категорію")])
            .with("categories.add_under", &[("en", "Add under"), ("de", "Hinzufügen unter"), ("fr", "Ajouter sous"), ("it", "Aggiungi sotto"), ("uk", "Додати в")])
            .with("categories.top_level", &[("en", "Top level"), ("de", "Oberste Ebene"), ("fr", "Niveau supérieur"), ("it", "Livello principale"), ("uk", "Верхній рівень")])
            .with("categories.new_name", &[("en", "New category name"), ("de", "Name der neuen Kategorie"), ("fr", "Nom de la nouvelle catégorie"), ("it", "Nome della nuova categoria"), ("uk", "Назва нової категорії")])
            .with("action.add", &[("en", "Add"), ("de", "Hinzufügen"), ("fr", "Ajouter"), ("it", "Aggiungi"), ("uk", "Додати")])
            .with("action.done", &[("en", "Done"), ("de", "Fertig"), ("fr", "Terminé"), ("it", "Fatto"), ("uk", "Готово")])
            // appearance
            .with("settings.appearance", &[("en", "Appearance"), ("de", "Darstellung"), ("fr", "Apparence"), ("it", "Aspetto"), ("uk", "Вигляд")])
            .with("appearance.light", &[("en", "Light"), ("de", "Hell"), ("fr", "Clair"), ("it", "Chiaro"), ("uk", "Світла")])
            .with("appearance.dark", &[("en", "Dark"), ("de", "Dunkel"), ("fr", "Sombre"), ("it", "Scuro"), ("uk", "Темна")])
            // security / app lock
            .with("settings.security", &[("en", "Security"), ("de", "Sicherheit"), ("fr", "Sécurité"), ("it", "Sicurezza"), ("uk", "Безпека")])
            .with("lock.desc", &[("en", "Require Face ID, Touch ID, or your passcode to open Saldo."), ("de", "Face ID, Touch ID oder Code zum Öffnen von Saldo verlangen."), ("fr", "Exiger Face ID, Touch ID ou votre code pour ouvrir Saldo."), ("it", "Richiedi Face ID, Touch ID o il codice per aprire Saldo."), ("uk", "Вимагати Face ID, Touch ID або код для відкриття Saldo.")])
            .with("lock.off", &[("en", "Off"), ("de", "Aus"), ("fr", "Désactivé"), ("it", "Disattivato"), ("uk", "Вимк.")])
            .with("lock.on", &[("en", "On"), ("de", "Ein"), ("fr", "Activé"), ("it", "Attivato"), ("uk", "Увімк.")])
            .with("lock.title", &[("en", "Saldo is locked"), ("de", "Saldo ist gesperrt"), ("fr", "Saldo est verrouillé"), ("it", "Saldo è bloccato"), ("uk", "Saldo заблоковано")])
            .with("lock.hint", &[("en", "Unlock with Face ID, Touch ID, or your passcode."), ("de", "Mit Face ID, Touch ID oder Code entsperren."), ("fr", "Déverrouillez avec Face ID, Touch ID ou votre code."), ("it", "Sblocca con Face ID, Touch ID o il codice."), ("uk", "Розблокуйте за допомогою Face ID, Touch ID або коду.")])
            .with("lock.unlock", &[("en", "Unlock"), ("de", "Entsperren"), ("fr", "Déverrouiller"), ("it", "Sblocca"), ("uk", "Розблокувати")])
            .with("lock.prompt", &[("en", "Unlock Saldo"), ("de", "Saldo entsperren"), ("fr", "Déverrouiller Saldo"), ("it", "Sblocca Saldo"), ("uk", "Розблокувати Saldo")])
            .with("err.cat_name", &[("en", "Give the category a name."), ("de", "Gib der Kategorie einen Namen."), ("fr", "Donnez un nom à la catégorie."), ("it", "Dai un nome alla categoria."), ("uk", "Дайте категорії назву.")])
            .with("settings.system", &[("en", "System"), ("de", "System"), ("fr", "Système"), ("it", "Sistema"), ("uk", "Системна")])
            // validation errors (returned as keys by validate_txn)
            .with("err.amount", &[("en", "Enter an amount greater than zero."), ("de", "Gib einen Betrag größer als null ein."), ("fr", "Saisissez un montant supérieur à zéro."), ("it", "Inserisci un importo maggiore di zero."), ("uk", "Введіть суму більше нуля.")])
            .with("err.account", &[("en", "Pick an account."), ("de", "Wähle ein Konto."), ("fr", "Choisissez un compte."), ("it", "Scegli un conto."), ("uk", "Виберіть рахунок.")])
            .with("err.dest", &[("en", "Pick a destination account."), ("de", "Wähle ein Zielkonto."), ("fr", "Choisissez un compte destinataire."), ("it", "Scegli un conto di destinazione."), ("uk", "Виберіть рахунок призначення.")])
            .with("err.twoaccounts", &[("en", "Pick two different accounts."), ("de", "Wähle zwei verschiedene Konten."), ("fr", "Choisissez deux comptes différents."), ("it", "Scegli due conti diversi."), ("uk", "Виберіть два різні рахунки.")])
            .with("err.category", &[("en", "Pick a category."), ("de", "Wähle eine Kategorie."), ("fr", "Choisissez une catégorie."), ("it", "Scegli una categoria."), ("uk", "Виберіть категорію.")])
            .with("err.save", &[("en", "Could not save — please try again."), ("de", "Speichern fehlgeschlagen — bitte erneut versuchen."), ("fr", "Échec de l'enregistrement — réessayez."), ("it", "Salvataggio non riuscito — riprova."), ("uk", "Не вдалося зберегти — спробуйте ще раз.")])
            .with("err.accname", &[("en", "Give the account a name."), ("de", "Gib dem Konto einen Namen."), ("fr", "Donnez un nom au compte."), ("it", "Dai un nome al conto."), ("uk", "Дайте рахунку назву.")])
    })
}

// ---- pure helpers (unit-tested) ----

fn parse_amount(s: &str) -> Option<f64> {
    let v = s.trim().replace(',', ".").parse::<f64>().ok()?;
    (v > 0.0).then_some(v)
}

fn pragma_int(json: &str) -> u32 {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .ok()
        .and_then(|rows| rows.first()?.get("user_version")?.as_str()?.parse().ok())
        .unwrap_or(0)
}

fn parse_accounts(json: &str) -> Vec<Account> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            Some(Account {
                id: r.get("id")?.as_str()?.parse().ok()?,
                name: r.get("name")?.as_str()?.to_string(),
                kind: AccountKind::from_db(r.get("kind").and_then(serde_json::Value::as_str).unwrap_or("asset")),
                opening: r.get("opening").and_then(serde_json::Value::as_str).unwrap_or("0").parse().unwrap_or(0.0),
            })
        })
        .collect()
}

fn parse_categories(json: &str) -> Vec<Category> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            Some(Category {
                id: r.get("id")?.as_str()?.parse().ok()?,
                name: r.get("name")?.as_str()?.to_string(),
                kind: r.get("kind")?.as_str()?.to_string(),
                parent_id: r.get("parent_id").and_then(serde_json::Value::as_str).and_then(|s| s.parse().ok()),
            })
        })
        .collect()
}

fn parse_txns(json: &str) -> Vec<Txn> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            Some(Txn {
                id: r.get("id")?.as_str()?.parse().ok()?,
                ts: r.get("ts")?.as_str()?.to_string(),
                kind: TxnKind::from_db(r.get("kind").and_then(serde_json::Value::as_str).unwrap_or("expense")),
                amount: r.get("amount")?.as_str()?.parse().ok()?,
                account_id: r.get("account_id")?.as_str()?.parse().ok()?,
                to_account_id: r.get("to_account_id").and_then(serde_json::Value::as_str).and_then(|s| s.parse().ok()),
                category: r.get("category").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                note: r.get("note").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn parse_recurring(json: &str) -> Vec<Recurring> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            Some(Recurring {
                id: r.get("id")?.as_str()?.parse().ok()?,
                kind: TxnKind::from_db(r.get("kind").and_then(serde_json::Value::as_str).unwrap_or("expense")),
                amount: r.get("amount")?.as_str()?.parse().ok()?,
                account_id: r.get("account_id")?.as_str()?.parse().ok()?,
                to_account_id: r.get("to_account_id").and_then(serde_json::Value::as_str).and_then(|s| s.parse().ok()),
                category: r.get("category").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                note: r.get("note").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                freq: Freq::from_db(r.get("freq").and_then(serde_json::Value::as_str).unwrap_or("daily")),
                next_date: r.get("next_date")?.as_str()?.to_string(),
                end_date: r.get("end_date").and_then(serde_json::Value::as_str).map(str::to_string),
            })
        })
        .collect()
}

/// Returns a translation **key** (not English) on failure, so the caller can localize the toast.
fn validate_txn(model: &Model) -> Result<(), &'static str> {
    if parse_amount(&model.draft_amount).is_none() {
        return Err("err.amount");
    }
    if model.draft_account.is_none() {
        return Err("err.account");
    }
    if model.draft_kind == TxnKind::Transfer {
        match model.draft_to_account {
            None => return Err("err.dest"),
            id if id == model.draft_account => return Err("err.twoaccounts"),
            _ => {}
        }
    } else if model.draft_category.trim().is_empty() {
        return Err("err.category");
    }
    Ok(())
}

fn in_period(ts: &str, period: Period, today: &str) -> bool {
    match period {
        Period::All => true,
        Period::Day => ts.get(..10) == Some(today),
        Period::Month => !today.is_empty() && ts.get(..7) == today.get(..7),
    }
}

fn period_totals(txns: &[Txn], period: Period, today: &str) -> (f64, f64) {
    let mut income = 0.0;
    let mut expense = 0.0;
    for t in txns.iter().filter(|t| in_period(&t.ts, period, today)) {
        match t.kind {
            TxnKind::Income => income += t.amount,
            TxnKind::Expense => expense += t.amount,
            TxnKind::Transfer => {}
        }
    }
    (income, expense)
}

fn balance(acc: &Account, txns: &[Txn]) -> f64 {
    let mut b = acc.opening;
    for t in txns {
        match t.kind {
            TxnKind::Income if t.account_id == acc.id => b += t.amount,
            TxnKind::Expense if t.account_id == acc.id => b -= t.amount,
            TxnKind::Transfer => {
                if t.account_id == acc.id {
                    b -= t.amount;
                }
                if t.to_account_id == Some(acc.id) {
                    b += t.amount;
                }
            }
            _ => {}
        }
    }
    b
}

fn net_worth(accounts: &[Account], txns: &[Txn]) -> (f64, f64, f64) {
    let mut assets = 0.0;
    let mut liabilities = 0.0;
    for a in accounts {
        let b = balance(a, txns);
        match a.kind {
            AccountKind::Asset => assets += b,
            AccountKind::Liability => liabilities += b,
        }
    }
    (assets, liabilities, assets - liabilities)
}

/// The last `n` `"YYYY-MM"` month tags, oldest→newest, ending at `today`'s month. Empty if `today`
/// isn't a parseable date. We walk back month by month (rolling the year over at January) then reverse.
fn month_tags(today: &str, n: usize) -> Vec<String> {
    let (Some(mut y), Some(mut m)) = (
        today.get(..4).and_then(|s| s.parse::<i32>().ok()),
        today.get(5..7).and_then(|s| s.parse::<i32>().ok()),
    ) else {
        return Vec::new();
    };
    let mut tags = Vec::with_capacity(n);
    for _ in 0..n {
        tags.push(format!("{y:04}-{m:02}"));
        m -= 1;
        if m == 0 {
            m = 12;
            y -= 1;
        }
    }
    tags.reverse();
    tags
}

/// Net worth (assets − liabilities) counting only transactions on or before the end of `tag`'s month.
/// `ts` is `"YYYY-MM-DD HH:MM:SS"`, so a lexical compare against `"{tag}-31 23:59:59"` is the cutoff.
fn net_worth_asof(accounts: &[Account], txns: &[Txn], tag: &str) -> f64 {
    let cutoff = format!("{tag}-31 23:59:59");
    let upto: Vec<Txn> = txns.iter().filter(|t| t.ts.as_str() <= cutoff.as_str()).cloned().collect();
    net_worth(accounts, &upto).2
}

/// `(income, expense)` totals for the single month `tag` (`"YYYY-MM"`).
fn monthly_totals(txns: &[Txn], tag: &str) -> (f64, f64) {
    let mut income = 0.0;
    let mut expense = 0.0;
    for t in txns.iter().filter(|t| t.ts.get(..7) == Some(tag)) {
        match t.kind {
            TxnKind::Income => income += t.amount,
            TxnKind::Expense => expense += t.amount,
            TxnKind::Transfer => {}
        }
    }
    (income, expense)
}

/// In-period transactions of `kind`, summed by the category **actually logged** (a subcategory shows as
/// itself, not rolled up to its parent) and sorted by amount descending.
fn category_breakdown(model: &Model, kind: TxnKind) -> Vec<(String, f64)> {
    let mut totals: Vec<(String, f64)> = Vec::new();
    for t in model.txns.iter().filter(|t| t.kind == kind && in_period(&t.ts, model.period, &model.today))
    {
        if t.category.is_empty() {
            continue;
        }
        match totals.iter_mut().find(|(n, _)| *n == t.category) {
            Some((_, amt)) => *amt += t.amount,
            None => totals.push((t.category.clone(), t.amount)),
        }
    }
    totals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    totals
}

/// A small flat palette; the Stats donut series and ranked list index into it so the colours line up.
fn palette(i: usize) -> Rgb {
    const COLORS: [(u8, u8, u8); 8] = [
        (0x2F, 0x80, 0xED), // blue
        (0xEB, 0x57, 0x57), // red
        (0x27, 0xAE, 0x60), // green
        (0xF2, 0xC9, 0x4C), // yellow
        (0x9B, 0x51, 0xE0), // purple
        (0xF2, 0x99, 0x4A), // orange
        (0x56, 0xCC, 0xF2), // cyan
        (0xBD, 0xBD, 0xBD), // grey
    ];
    let (r, g, b) = COLORS[i % COLORS.len()];
    Rgb::new(r, g, b)
}

fn group_by_day<'a>(items: &[&'a Txn]) -> Vec<(&'a str, Vec<&'a Txn>)> {
    let mut out: Vec<(&str, Vec<&Txn>)> = Vec::new();
    for &t in items {
        match out.last_mut() {
            Some((day, group)) if *day == t.day() => group.push(t),
            _ => out.push((t.day(), vec![t])),
        }
    }
    out
}

fn acct_name(accounts: &[Account], id: u32) -> &str {
    accounts.iter().find(|a| a.id == id).map_or("?", |a| a.name.as_str())
}

/// The parent category whose subcategory row should be open, given the selected category name:
/// the selected category's parent (if it's a subcategory) or itself (if it's a top-level).
fn open_parent(categories: &[Category], kind: &str, selected: &str) -> Option<u32> {
    categories
        .iter()
        .find(|c| c.kind == kind && c.name == selected)
        .map(|c| c.parent_id.unwrap_or(c.id))
}

/// A sensible default base currency for a freshly-detected locale (the user can change it in Settings
/// later). Region-implied where obvious, otherwise the euro.
fn default_currency(locale: Locale) -> Currency {
    match locale {
        Locale::UkUa => Currency::Uah,
        Locale::EnUs => Currency::Usd,
        Locale::EnGb => Currency::Gbp,
        Locale::DeCh | Locale::FrCh | Locale::ItCh => Currency::Chf,
        _ => Currency::Eur,
    }
}

fn money(model: &Model, v: f64) -> String {
    format_currency(v, model.currency, model.locale)
}

fn signed(model: &Model, t: &Txn) -> String {
    match t.kind {
        TxnKind::Income => format!("+{}", money(model, t.amount)),
        TxnKind::Expense => format!("−{}", money(model, t.amount)),
        TxnKind::Transfer => money(model, t.amount),
    }
}

/// Parse a `"YYYY-MM-DD"` prefix into `(year, month, day)`.
fn parse_ymd(s: &str) -> Option<(i32, u32, u32)> {
    Some((s.get(..4)?.parse().ok()?, s.get(5..7)?.parse().ok()?, s.get(8..10)?.parse().ok()?))
}

/// Format a `"YYYY-MM-DD"` date in the model's locale, falling back to the raw string.
fn fmt_date(model: &Model, ymd: &str) -> String {
    match parse_ymd(ymd) {
        Some((y, m, d)) => format_date(y, m, d, model.locale),
        None => ymd.to_string(),
    }
}

// ---- date arithmetic (for advancing recurring rules) ----

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`); handles any proleptic Gregorian date.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(if m <= 2 { y - 1 } else { y });
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    // m ∈ 1..=12, d ∈ 1..=31 — both small and positive, so the casts are safe.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (i32::try_from(if m <= 2 { y + 1 } else { y }).unwrap_or(0), m as u32, d as u32)
}

fn ymd_str(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// `"YYYY-MM-DD"` shifted by `n` days (negative shifts back).
fn add_days(ymd: &str, n: i64) -> String {
    let Some((y, m, d)) = parse_ymd(ymd) else { return ymd.to_string() };
    let (ny, nm, nd) = civil_from_days(days_from_civil(y, m, d) + n);
    ymd_str(ny, nm, nd)
}

/// The next occurrence after `ymd` for `freq`. Monthly keeps the day-of-month, clamped to the month's
/// length (e.g. Jan 31 → Feb 28/29).
fn advance(ymd: &str, freq: Freq) -> String {
    match freq {
        Freq::Daily => add_days(ymd, 1),
        Freq::Weekly => add_days(ymd, 7),
        Freq::Monthly => {
            let Some((y, m, d)) = parse_ymd(ymd) else { return ymd.to_string() };
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            ymd_str(ny, nm, d.min(days_in_month(ny, nm)))
        }
    }
}

/// A transaction a recurring rule is due to post, on a given date.
struct DuePost {
    kind: TxnKind,
    amount: f64,
    account_id: u32,
    to_account_id: Option<u32>,
    category: String,
    note: String,
    date: String,
}

/// Catch-up materialization: every occurrence on or before `today` (and within `end_date`) becomes a
/// post, and each rule that fired gets its advanced `next_date`. Pure — the caller turns these into SQL.
fn due_posts(rules: &[Recurring], today: &str) -> (Vec<DuePost>, Vec<(u32, String)>) {
    let mut posts = Vec::new();
    let mut updates = Vec::new();
    if today.is_empty() {
        return (posts, updates);
    }
    for r in rules {
        let mut next = r.next_date.clone();
        let mut fired = false;
        // bounded so a long-dormant daily rule can't post forever in one launch
        for _ in 0..400 {
            if next.as_str() > today || r.end_date.as_deref().is_some_and(|e| next.as_str() > e) {
                break;
            }
            posts.push(DuePost {
                kind: r.kind,
                amount: r.amount,
                account_id: r.account_id,
                to_account_id: r.to_account_id,
                category: r.category.clone(),
                note: r.note.clone(),
                date: next.clone(),
            });
            next = advance(&next, r.freq);
            fired = true;
        }
        if fired {
            updates.push((r.id, next));
        }
    }
    (posts, updates)
}

// ---- view pieces ----

fn period_seg(current: Period, p: Period, label: impl Into<String>) -> Segment {
    segment(label, current == p, Msg::SetPeriod(p))
}

/// The translated Day / Month / All period control, shared by the Bills and Stats tabs.
fn period_segments(model: &Model) -> Vec<Segment> {
    vec![
        period_seg(model.period, Period::Day, tr(model, "period.day")),
        period_seg(model.period, Period::Month, tr(model, "period.month")),
        period_seg(model.period, Period::All, tr(model, "period.all")),
    ]
}

/// One selectable row in a settings list — a full-width tappable tile, accented with a trailing check
/// when it's the current choice.
fn choice_row(label: impl Into<String>, selected: bool, msg: Msg) -> Widget {
    card_button(
        row(vec![text(label), spacer(Spacing::Md), text(if selected { "✓" } else { "" })]),
        if selected { CardStyle::Brand } else { CardStyle::Outlined },
        msg,
    )
}

/// A compact, collapsible "select" (the framework has no native dropdown): a header row showing the
/// current value that expands the `options` inline when tapped, and collapses once a choice is made.
fn select_field(title: String, current: String, open: bool, toggle: Msg, options: Vec<Widget>) -> Widget {
    let header = card_button(
        row(vec![
            text(title),
            spacer(Spacing::Md),
            caption(current),
            text(if open { " ▴" } else { " ▾" }),
        ]),
        CardStyle::Outlined,
        toggle,
    );
    if open {
        let mut items = vec![header, spacer(Spacing::Xs)];
        items.extend(options);
        column(items)
    } else {
        header
    }
}

/// The endonym (a language's own name) for a code — shown untranslated so anyone can spot their language.
fn endonym(code: &str) -> &'static str {
    match code {
        "de" => "Deutsch",
        "fr" => "Français",
        "it" => "Italiano",
        "uk" => "Українська",
        _ => "English",
    }
}

/// The Settings tab — language + currency pickers (vertical lists), the scheduled rules, and the Data card.
fn settings(model: &Model) -> Widget {
    let chosen = model.lang_override.as_deref();
    let lang_current = chosen.map_or_else(|| tr(model, "settings.system"), |c| endonym(c).to_string());
    let mut lang_opts =
        vec![choice_row(tr(model, "settings.system"), chosen.is_none(), Msg::SetLang(None))];
    for code in SUPPORTED {
        lang_opts.push(choice_row(endonym(code), chosen == Some(code), Msg::SetLang(Some(code.to_string()))));
    }
    let language = select_field(
        tr(model, "settings.language"),
        lang_current,
        model.lang_open,
        Msg::ToggleLangPicker,
        lang_opts,
    );

    let cur_opts: Vec<Widget> = CURRENCIES
        .iter()
        .map(|(cur, code)| choice_row(*code, model.currency == *cur, Msg::SetCurrency(*cur)))
        .collect();
    let currency = select_field(
        tr(model, "settings.currency"),
        currency_code(model.currency).to_string(),
        model.cur_open,
        Msg::ToggleCurPicker,
        cur_opts,
    );

    let mut scheduled = vec![subtitle(tr(model, "settings.scheduled")), spacer(Spacing::Sm)];
    if model.recurring.is_empty() {
        scheduled.push(caption(tr(model, "recurring.empty")));
    } else {
        for r in &model.recurring {
            scheduled.push(recurring_row(model, r));
            scheduled.push(spacer(Spacing::Xs));
        }
    }

    let data = card(
        column(vec![
            subtitle(tr(model, "settings.data")),
            caption(tr(model, "data.hint")),
            spacer(Spacing::Sm),
            button(tr(model, "data.export_csv"), ButtonStyle::Filled, Msg::ExportCsv),
            button(tr(model, "data.backup"), ButtonStyle::Filled, Msg::Backup),
            button(tr(model, "data.restore"), ButtonStyle::Outlined, Msg::Restore),
        ]),
        CardStyle::Elevated,
    );

    let categories = card(
        column(vec![
            subtitle(tr(model, "settings.categories")),
            spacer(Spacing::Sm),
            button(tr(model, "categories.manage"), ButtonStyle::Filled, Msg::StartManageCategories),
        ]),
        CardStyle::Elevated,
    );

    let security = card(
        column(vec![
            subtitle(tr(model, "settings.security")),
            caption(tr(model, "lock.desc")),
            spacer(Spacing::Sm),
            segmented(vec![
                segment(tr(model, "lock.off"), !model.lock_enabled, Msg::SetLock(false)),
                segment(tr(model, "lock.on"), model.lock_enabled, Msg::SetLock(true)),
            ]),
        ]),
        CardStyle::Elevated,
    );

    let appearance = card(
        column(vec![
            subtitle(tr(model, "settings.appearance")),
            spacer(Spacing::Sm),
            segmented(vec![
                segment(tr(model, "appearance.light"), !model.dark, Msg::SetDark(false)),
                segment(tr(model, "appearance.dark"), model.dark, Msg::SetDark(true)),
            ]),
        ]),
        CardStyle::Elevated,
    );

    column(vec![
        spacer(Spacing::Md),
        appearance,
        spacer(Spacing::Md),
        language,
        spacer(Spacing::Sm),
        currency,
        spacer(Spacing::Md),
        categories,
        spacer(Spacing::Md),
        security,
        spacer(Spacing::Md),
        column(scheduled),
        spacer(Spacing::Md),
        data,
    ])
}

fn recurring_row(model: &Model, r: &Recurring) -> Widget {
    let label = match r.kind {
        TxnKind::Transfer => format!(
            "{} → {}",
            acct_name(&model.accounts, r.account_id),
            r.to_account_id.map_or("?", |id| acct_name(&model.accounts, id)),
        ),
        _ => r.category.clone(),
    };
    let detail = format!(
        "{} · {} {}",
        tr(model, r.freq.key()),
        tr(model, "recurring.next"),
        fmt_date(model, &r.next_date),
    );
    // Swipe to delete the rule — consistent with the transactions, accounts, and categories lists.
    swipe_action(
        card(
            row(vec![
                column(vec![text(format!("{} {label}", money(model, r.amount))), caption(detail)]),
                spacer(Spacing::Md),
            ]),
            CardStyle::Filled,
        ),
        vec![(tr(model, "delete"), Tone::Danger, Msg::DeleteRecurring(r.id))],
    )
}

fn bills(model: &Model) -> Widget {
    let shown: Vec<&Txn> =
        model.txns.iter().filter(|t| in_period(&t.ts, model.period, &model.today)).collect();
    let (income, expense) = period_totals(&model.txns, model.period, &model.today);

    let header = card(
        column(vec![
            segmented(period_segments(model)),
            spacer(Spacing::Sm),
            row(vec![caption(tr(model, "income")), spacer(Spacing::Md), emphasis(money(model, income))]),
            row(vec![caption(tr(model, "expense")), spacer(Spacing::Md), emphasis(money(model, expense))]),
            divider(),
            row(vec![caption(tr(model, "net")), spacer(Spacing::Md), emphasis(money(model, income - expense))]),
        ]),
        CardStyle::Filled,
    );

    if shown.is_empty() {
        return column(vec![header, spacer(Spacing::Xl), caption(tr(model, "bills.empty"))]);
    }

    // Each day is a light header followed by one swipe-to-delete card per transaction (web, which has no
    // swipe gesture, renders the Delete action inline). Cleaner than cramming a Delete button on every row.
    let mut sections = vec![header, spacer(Spacing::Md)];
    for (day, items) in group_by_day(&shown) {
        sections.push(row(vec![subtitle(fmt_date(model, day)), spacer(Spacing::Md)]));
        sections.push(spacer(Spacing::Xs));
        for t in items {
            sections.push(swipe_action(
                card(txn_row(model, t), CardStyle::Filled),
                vec![(tr(model, "delete"), Tone::Danger, Msg::Delete(t.id))],
            ));
            sections.push(spacer(Spacing::Xs));
        }
        sections.push(spacer(Spacing::Sm));
    }
    column(sections)
}

fn txn_row(model: &Model, t: &Txn) -> Widget {
    let title = match t.kind {
        TxnKind::Transfer => format!(
            "{} → {}",
            acct_name(&model.accounts, t.account_id),
            t.to_account_id.map_or("?", |id| acct_name(&model.accounts, id)),
        ),
        _ => t.category.clone(),
    };
    let label = if t.note.is_empty() {
        column(vec![text(title)])
    } else {
        column(vec![text(title), caption(t.note.clone())])
    };
    row(vec![label, spacer(Spacing::Md), emphasis(signed(model, t))])
}

fn assets(model: &Model) -> Widget {
    let (assets_total, liabilities, net) = net_worth(&model.accounts, &model.txns);
    let summary = card(
        column(vec![
            row(vec![caption(tr(model, "assets.assets")), spacer(Spacing::Md), emphasis(money(model, assets_total))]),
            row(vec![caption(tr(model, "assets.liabilities")), spacer(Spacing::Md), emphasis(money(model, liabilities))]),
            divider(),
            row(vec![text(tr(model, "networth")), spacer(Spacing::Md), emphasis(money(model, net))]),
        ]),
        CardStyle::Filled,
    );

    if model.accounts.is_empty() {
        return column(vec![summary, spacer(Spacing::Xl), caption(tr(model, "assets.empty"))]);
    }

    // Tap a row to edit the account; swipe to delete it.
    let mut rows = vec![summary, spacer(Spacing::Md)];
    for a in &model.accounts {
        let tag = if a.kind == AccountKind::Liability { tr(model, "liability.tag") } else { String::new() };
        let content = row(vec![
            text(format!("{}{tag}", a.name)),
            spacer(Spacing::Md),
            emphasis(money(model, balance(a, &model.txns))),
        ]);
        rows.push(swipe_action(
            card_button(content, CardStyle::Filled, Msg::EditAccount(a.id)),
            vec![(tr(model, "delete"), Tone::Danger, Msg::DeleteAccount(a.id))],
        ));
        rows.push(spacer(Spacing::Xs));
    }
    column(rows)
}

/// The Stats tab: a category-breakdown donut (income/expense toggle, current period) with a ranked
/// list, a net-worth trend line, and a monthly income-vs-expense bar chart — all from the `Chart` widget.
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)] // money f64 → chart f32 is fine here
fn stats(model: &Model) -> Widget {
    if model.txns.is_empty() {
        return column(vec![spacer(Spacing::Xl), caption(tr(model, "stats.empty"))]);
    }

    let kind = model.stats_kind;
    let controls = card(
        column(vec![
            segmented(period_segments(model)),
            spacer(Spacing::Sm),
            segmented(vec![
                segment(tr(model, "expense"), kind == TxnKind::Expense, Msg::SetStatsKind(TxnKind::Expense)),
                segment(tr(model, "income"), kind == TxnKind::Income, Msg::SetStatsKind(TxnKind::Income)),
            ]),
        ]),
        CardStyle::Filled,
    );

    // 1. Category breakdown — donut + ranked list.
    let breakdown = category_breakdown(model, kind);
    let total: f64 = breakdown.iter().map(|(_, a)| a).sum();
    let breakdown_card = if breakdown.is_empty() {
        card(
            column(vec![
                subtitle(tr(model, "stats.bycategory")),
                spacer(Spacing::Sm),
                caption(tr(model, if kind == TxnKind::Income { "stats.no_income" } else { "stats.no_expense" })),
            ]),
            CardStyle::Elevated,
        )
    } else {
        let donut = donut_chart(
            breakdown
                .iter()
                .enumerate()
                .map(|(i, (name, amt))| {
                    ChartSeries::new(name.clone(), vec![*amt as f32]).with_color(palette(i))
                })
                .collect(),
        );
        let mut items = vec![subtitle(tr(model, "stats.bycategory")), spacer(Spacing::Sm), donut, divider()];
        for (name, amt) in &breakdown {
            let pct = if total > 0.0 { amt / total * 100.0 } else { 0.0 };
            items.push(row(vec![
                text(name.clone()),
                spacer(Spacing::Md),
                emphasis(money(model, *amt)),
                caption(format!("{pct:.0}%")),
            ]));
        }
        card(column(items), CardStyle::Elevated)
    };

    // 2. Net-worth trend (last 6 months).
    let tags = month_tags(&model.today, 6);
    let labels: Vec<String> = tags.iter().map(|t| t.get(5..7).unwrap_or(t).to_string()).collect();
    let trend = tags.iter().map(|t| net_worth_asof(&model.accounts, &model.txns, t) as f32).collect();
    let trend_card = card(
        column(vec![
            subtitle(tr(model, "stats.trend")),
            spacer(Spacing::Sm),
            chart(vec![ChartSeries::new(tr(model, "networth"), trend)], labels.clone(), ChartStyle::Line, true, false),
        ]),
        CardStyle::Elevated,
    );

    // 3. Monthly income vs expense (last 6 months).
    let (mut inc, mut exp) = (Vec::new(), Vec::new());
    for t in &tags {
        let (i, e) = monthly_totals(&model.txns, t);
        inc.push(i as f32);
        exp.push(e as f32);
    }
    let bars_card = card(
        column(vec![
            subtitle(tr(model, "stats.monthly")),
            spacer(Spacing::Sm),
            chart(
                vec![
                    ChartSeries::new(tr(model, "income"), inc).with_color(palette(2)),
                    ChartSeries::new(tr(model, "expense"), exp).with_color(palette(1)),
                ],
                labels,
                ChartStyle::Bar,
                true,
                true,
            ),
        ]),
        CardStyle::Elevated,
    );

    column(vec![
        controls,
        spacer(Spacing::Md),
        breakdown_card,
        spacer(Spacing::Md),
        trend_card,
        spacer(Spacing::Md),
        bars_card,
    ])
}

fn account_chips(model: &Model, selected: Option<u32>, on: fn(u32) -> Msg) -> Widget {
    // A horizontal rail so the chips keep their width and scroll instead of being crushed onto one row.
    scroller(model.accounts.iter().map(|a| chip(a.name.clone(), selected == Some(a.id), on(a.id))).collect())
}

/// A two-level category picker. Top-level chips first; a `›` marks ones with subcategories. Tapping a
/// top-level selects it *and* reveals its subcategories (labelled with the parent name) on a second rail,
/// each directly selectable — so you can log to either a category or one of its subcategories.
fn category_picker(model: &Model) -> Widget {
    let kind = model.draft_kind.category_kind();
    let sel = &model.draft_category;
    let open = open_parent(&model.categories, kind, sel);
    let has_kids =
        |id: u32| model.categories.iter().any(|c| c.kind == kind && c.parent_id == Some(id));

    let tops: Vec<Widget> = model
        .categories
        .iter()
        .filter(|c| c.kind == kind && c.parent_id.is_none())
        .map(|c| {
            // highlight a parent while we're inside its subcategory group, too
            let active = &c.name == sel || open == Some(c.id);
            let label = if has_kids(c.id) { format!("{} ›", c.name) } else { c.name.clone() };
            chip(label, active, Msg::SetCategory(c.name.clone()))
        })
        .collect();

    let mut items = vec![caption(tr(model, "field.category")), scroller(tops)];
    if let Some(pid) = open {
        let kids: Vec<Widget> = model
            .categories
            .iter()
            .filter(|c| c.kind == kind && c.parent_id == Some(pid))
            .map(|c| chip(c.name.clone(), &c.name == sel, Msg::SetCategory(c.name.clone())))
            .collect();
        if !kids.is_empty() {
            let parent = model.categories.iter().find(|c| c.id == pid).map_or("", |c| c.name.as_str());
            items.push(caption(format!("{parent}:")));
            items.push(scroller(kids));
        }
    }
    column(items)
}

/// The inline validation error (a danger badge) for the open sheet, if any — always visible, unlike a
/// toast that the keyboard can cover.
fn error_banner(model: &Model) -> Vec<Widget> {
    model
        .form_error
        .map_or_else(Vec::new, |key| vec![badge(tr(model, key), Tone::Danger), spacer(Spacing::Sm)])
}

/// A Cancel + Save row, so every sheet has a visible way out even with the keyboard up.
fn save_bar(model: &Model, save: Msg, cancel: Msg) -> Widget {
    row(vec![
        button(tr(model, "action.cancel"), ButtonStyle::Outlined, cancel),
        button(tr(model, "action.save"), ButtonStyle::Filled, save),
    ])
}

fn txn_sheet(model: &Model) -> Widget {
    let mut items = vec![
        segmented(vec![
            segment(tr(model, "expense"), model.draft_kind == TxnKind::Expense, Msg::SetKind(TxnKind::Expense)),
            segment(tr(model, "income"), model.draft_kind == TxnKind::Income, Msg::SetKind(TxnKind::Income)),
            segment(tr(model, "type.transfer"), model.draft_kind == TxnKind::Transfer, Msg::SetKind(TxnKind::Transfer)),
        ]),
        spacer(Spacing::Sm),
        decimal_field("amount", tr(model, "field.amount"), model.draft_amount.clone()),
        row(vec![
            caption(tr(model, "field.date")),
            spacer(Spacing::Md),
            text(fmt_date(model, &model.draft_date)),
            button(tr(model, "action.change"), ButtonStyle::Text, Msg::PickDate),
        ]),
        caption(tr(model, if model.draft_kind == TxnKind::Transfer { "field.from" } else { "field.account" })),
        account_chips(model, model.draft_account, Msg::SetAccount),
    ];
    if model.draft_kind == TxnKind::Transfer {
        items.push(caption(tr(model, "field.to")));
        items.push(account_chips(model, model.draft_to_account, Msg::SetToAccount));
    } else {
        items.push(category_picker(model));
    }
    items.push(text_field("note", tr(model, "field.note"), model.draft_note.clone()));
    // Repeat: a one-off (None) or a recurring rule. The rule's first occurrence posts immediately.
    items.push(caption(tr(model, "field.repeat")));
    items.push(segmented(vec![
        segment(tr(model, "freq.once"), model.draft_freq.is_none(), Msg::SetFreq(None)),
        segment(tr(model, "freq.daily"), model.draft_freq == Some(Freq::Daily), Msg::SetFreq(Some(Freq::Daily))),
        segment(tr(model, "freq.weekly"), model.draft_freq == Some(Freq::Weekly), Msg::SetFreq(Some(Freq::Weekly))),
        segment(tr(model, "freq.monthly"), model.draft_freq == Some(Freq::Monthly), Msg::SetFreq(Some(Freq::Monthly))),
    ]));
    items.push(spacer(Spacing::Md));
    items.extend(error_banner(model));
    items.push(save_bar(model, Msg::Save, Msg::CancelAdd));
    column(items)
}

fn account_sheet(model: &Model) -> Widget {
    let mut items = vec![
        text_field("acc_name", tr(model, "field.accname"), model.acc_name.clone()),
        segmented(vec![
            segment(tr(model, "kind.asset"), model.acc_kind == AccountKind::Asset, Msg::SetAccKind(AccountKind::Asset)),
            segment(tr(model, "kind.liability"), model.acc_kind == AccountKind::Liability, Msg::SetAccKind(AccountKind::Liability)),
        ]),
        decimal_field("acc_opening", tr(model, "field.opening"), model.acc_opening.clone()),
        spacer(Spacing::Md),
    ];
    items.extend(error_banner(model));
    items.push(save_bar(model, Msg::SaveAccount, Msg::CancelAddAccount));
    column(items)
}

/// The category editor (a full screen). Done to leave, an income/expense toggle, an Add button, then a
/// flat indented list of categories (subcategories under their parent) — each a card you **tap to
/// rename** and **swipe to delete**, exactly like the accounts and transactions lists.
fn category_manager(model: &Model) -> Widget {
    let kind = model.cat_kind.category_kind();
    let tops: Vec<&Category> =
        model.categories.iter().filter(|c| c.kind == kind && c.parent_id.is_none()).collect();

    let mut items = vec![
        button(tr(model, "action.done"), ButtonStyle::Filled, Msg::CancelManageCategories),
        spacer(Spacing::Sm),
        segmented(vec![
            segment(tr(model, "expense"), model.cat_kind == TxnKind::Expense, Msg::SetCatKind(TxnKind::Expense)),
            segment(tr(model, "income"), model.cat_kind == TxnKind::Income, Msg::SetCatKind(TxnKind::Income)),
        ]),
        spacer(Spacing::Sm),
        button(tr(model, "categories.add"), ButtonStyle::Outlined, Msg::StartAddCategory),
        spacer(Spacing::Sm),
    ];
    for top in &tops {
        items.push(cat_row(model, top, false));
        items.push(spacer(Spacing::Xs));
        for sub in model.categories.iter().filter(|c| c.kind == kind && c.parent_id == Some(top.id)) {
            items.push(cat_row(model, sub, true));
            items.push(spacer(Spacing::Xs));
        }
    }
    column(items)
}

/// One category row: a tap-to-rename card with swipe-to-delete (subcategories shown indented).
fn cat_row(model: &Model, c: &Category, sub: bool) -> Widget {
    let label = if sub { format!("↳ {}", c.name) } else { c.name.clone() };
    let content = if sub { row(vec![caption(label)]) } else { row(vec![text(label)]) };
    swipe_action(
        card_button(content, CardStyle::Filled, Msg::EditCategory(c.id)),
        vec![(tr(model, "delete"), Tone::Danger, Msg::DeleteCategory(c.id))],
    )
}

/// The add/edit category sheet: a name field (plus a parent picker when adding), the same shape as the
/// account sheet. Editing renames; adding nests under the chosen parent for the current income/expense set.
fn category_sheet(model: &Model) -> Widget {
    let mut items =
        vec![text_field("cat_name", tr(model, "categories.new_name"), model.cat_name.clone())];
    if model.editing_category.is_none() {
        let kind = model.cat_kind.category_kind();
        let mut parents = vec![chip(
            tr(model, "categories.top_level"),
            model.cat_parent.is_none(),
            Msg::SetCatParent(None),
        )];
        for c in model.categories.iter().filter(|c| c.kind == kind && c.parent_id.is_none()) {
            parents.push(chip(c.name.clone(), model.cat_parent == Some(c.id), Msg::SetCatParent(Some(c.id))));
        }
        items.push(caption(tr(model, "categories.add_under")));
        items.push(scroller(parents));
    }
    items.push(spacer(Spacing::Md));
    items.extend(error_banner(model));
    items.push(save_bar(model, Msg::SaveCategory, Msg::CancelCatSheet));
    column(items)
}

/// The Crux app the FFI + codegen target — `MobilerShell` over `SaldoApp`.
pub type App = MobilerShell<SaldoApp>;

#[cfg(test)]
mod test {
    use super::*;

    fn cats() -> Vec<Category> {
        vec![
            Category { id: 1, name: "Food & Drink".into(), kind: "expense".into(), parent_id: None },
            Category { id: 2, name: "Transport".into(), kind: "expense".into(), parent_id: None },
            Category { id: 3, name: "Groceries".into(), kind: "expense".into(), parent_id: Some(1) },
            Category { id: 4, name: "Coffee".into(), kind: "expense".into(), parent_id: Some(1) },
            Category { id: 5, name: "Salary".into(), kind: "income".into(), parent_id: None },
        ]
    }

    #[test]
    fn migration_targets_version_5() {
        assert_eq!(*migrations_from(0).last().unwrap(), "PRAGMA user_version = 5");
        // a v4 device only needs the v5 (setting) block — not the earlier ones
        let from4 = migrations_from(4);
        assert_eq!(*from4.last().unwrap(), "PRAGMA user_version = 5");
        assert!(from4.iter().any(|s| s.contains("CREATE TABLE IF NOT EXISTS setting")));
        assert!(from4.iter().all(|s| !s.contains("CREATE TABLE IF NOT EXISTS recurring")));
        assert!(migrations_from(5).is_empty());
    }

    #[test]
    fn currency_codes_round_trip() {
        for (cur, code) in CURRENCIES {
            assert_eq!(currency_code(cur), code);
            assert_eq!(currency_from_code(code), Some(cur));
        }
        assert_eq!(currency_from_code("XYZ"), None);
    }

    #[test]
    fn parses_settings_rows() {
        let json = r#"[{"key":"lang","value":"uk"},{"key":"currency","value":"EUR"}]"#;
        let s = parse_settings(json);
        assert_eq!(s, vec![("lang".to_string(), "uk".to_string()), ("currency".to_string(), "EUR".to_string())]);
    }

    #[test]
    fn date_arithmetic_advances_correctly() {
        assert_eq!(add_days("2026-06-10", 1), "2026-06-11");
        assert_eq!(add_days("2026-12-31", 1), "2027-01-01"); // year rollover
        assert_eq!(add_days("2026-03-01", -1), "2026-02-28"); // 2026 isn't a leap year
        assert_eq!(advance("2026-06-10", Freq::Daily), "2026-06-11");
        assert_eq!(advance("2026-06-10", Freq::Weekly), "2026-06-17");
        assert_eq!(advance("2026-06-10", Freq::Monthly), "2026-07-10");
        assert_eq!(advance("2026-12-15", Freq::Monthly), "2027-01-15"); // month + year rollover
        assert_eq!(advance("2026-01-31", Freq::Monthly), "2026-02-28"); // clamp to month length
        assert_eq!(advance("2028-01-31", Freq::Monthly), "2028-02-29"); // 2028 is a leap year
    }

    fn rule(id: u32, freq: Freq, next: &str, end: Option<&str>) -> Recurring {
        Recurring {
            id, kind: TxnKind::Expense, amount: 10.0, account_id: 1, to_account_id: None,
            category: "Coffee".into(), note: String::new(), freq,
            next_date: next.into(), end_date: end.map(str::to_string),
        }
    }

    #[test]
    fn due_posts_catches_up_and_advances() {
        // a monthly rule due since March, today June → posts Mar/Apr/May/Jun, next advances to July
        let (posts, updates) = due_posts(&[rule(7, Freq::Monthly, "2026-03-10", None)], "2026-06-15");
        assert_eq!(posts.len(), 4);
        assert_eq!(posts[0].date, "2026-03-10");
        assert_eq!(posts[3].date, "2026-06-10");
        assert_eq!(updates, vec![(7, "2026-07-10".to_string())]);
    }

    #[test]
    fn due_posts_respects_end_date_and_future_rules() {
        // end_date in April caps a monthly rule at Mar/Apr even though today is June
        let (posts, _) = due_posts(&[rule(1, Freq::Monthly, "2026-03-10", Some("2026-04-30"))], "2026-06-15");
        assert_eq!(posts.len(), 2);
        // a rule whose next_date is in the future posts nothing and isn't advanced
        let (posts, updates) = due_posts(&[rule(2, Freq::Daily, "2026-07-01", None)], "2026-06-15");
        assert!(posts.is_empty());
        assert!(updates.is_empty());
        // no `today` yet → nothing happens
        assert_eq!(due_posts(&[rule(3, Freq::Daily, "2026-06-01", None)], "").0.len(), 0);
    }

    #[test]
    fn parses_categories_with_parents() {
        let json = r#"[
            {"id":"1","name":"Food & Drink","kind":"expense","sort":"1"},
            {"id":"3","name":"Groceries","kind":"expense","parent_id":"1","sort":"1"}
        ]"#;
        let cs = parse_categories(json);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].parent_id, None);
        assert_eq!(cs[1].parent_id, Some(1));
    }

    #[test]
    fn open_parent_resolves_for_top_and_sub() {
        let cs = cats();
        assert_eq!(open_parent(&cs, "expense", "Food & Drink"), Some(1)); // top-level → itself
        assert_eq!(open_parent(&cs, "expense", "Groceries"), Some(1)); // sub → its parent
        assert_eq!(open_parent(&cs, "expense", "nope"), None);
    }

    #[test]
    fn validate_txn_reports_each_missing_required_field() {
        // every required-field failure returns a specific (translatable) error key
        let mut m = Model::default();
        assert_eq!(validate_txn(&m), Err("err.amount")); // no amount
        m.draft_amount = "5".into();
        assert_eq!(validate_txn(&m), Err("err.account")); // no account
        m.draft_account = Some(1);
        assert_eq!(validate_txn(&m), Err("err.category")); // expense/income needs a category
        m.draft_category = "Coffee".into();
        assert!(validate_txn(&m).is_ok());
        // transfer: ignores category, needs a destination, and it must differ from the source
        m.draft_kind = TxnKind::Transfer;
        m.draft_category.clear();
        assert_eq!(validate_txn(&m), Err("err.dest"));
        m.draft_to_account = Some(1);
        assert_eq!(validate_txn(&m), Err("err.twoaccounts"));
        m.draft_to_account = Some(2);
        assert!(validate_txn(&m).is_ok());
    }

    #[test]
    fn balances_and_net_worth() {
        let accounts = vec![
            Account { id: 1, name: "Cash".into(), kind: AccountKind::Asset, opening: 100.0 },
            Account { id: 2, name: "Bank".into(), kind: AccountKind::Asset, opening: 0.0 },
        ];
        let txns = vec![
            Txn { id: 1, ts: "2026-06-10 12:00:00".into(), kind: TxnKind::Expense, amount: 12.5,
                  account_id: 1, to_account_id: None, category: "Coffee".into(), note: String::new() },
            Txn { id: 2, ts: "2026-06-10 12:00:00".into(), kind: TxnKind::Transfer, amount: 30.0,
                  account_id: 1, to_account_id: Some(2), category: String::new(), note: String::new() },
        ];
        assert!((balance(&accounts[0], &txns) - 57.5).abs() < 1e-9);
        assert!((balance(&accounts[1], &txns) - 30.0).abs() < 1e-9);
        assert!((net_worth(&accounts, &txns).2 - 87.5).abs() < 1e-9);
    }

    fn expense(id: u32, ts: &str, amount: f64, category: &str) -> Txn {
        Txn { id, ts: ts.into(), kind: TxnKind::Expense, amount, account_id: 1,
              to_account_id: None, category: category.into(), note: String::new() }
    }

    #[test]
    fn month_tags_walk_back_with_year_rollover() {
        assert_eq!(month_tags("2026-06-10", 3), ["2026-04", "2026-05", "2026-06"]);
        // crossing a year boundary
        assert_eq!(month_tags("2026-01-15", 3), ["2025-11", "2025-12", "2026-01"]);
        assert_eq!(month_tags("2026-06-10", 1), ["2026-06"]);
        assert!(month_tags("2026-06-10", 0).is_empty());
        assert!(month_tags("", 6).is_empty());
    }

    #[test]
    fn net_worth_asof_respects_the_month_cutoff() {
        let accounts =
            vec![Account { id: 1, name: "Cash".into(), kind: AccountKind::Asset, opening: 100.0 }];
        let txns = vec![
            expense(1, "2026-05-20 12:00:00", 10.0, "Coffee"),
            expense(2, "2026-06-05 12:00:00", 25.0, "Coffee"),
        ];
        // only the May expense counts by end of May; both count by end of June
        assert!((net_worth_asof(&accounts, &txns, "2026-05") - 90.0).abs() < 1e-9);
        assert!((net_worth_asof(&accounts, &txns, "2026-06") - 65.0).abs() < 1e-9);
    }

    #[test]
    fn category_breakdown_keeps_subcategories_distinct_and_sorts() {
        let model = Model {
            categories: cats(),
            today: "2026-06-10".into(),
            period: Period::All,
            txns: vec![
                expense(1, "2026-06-01 12:00:00", 4.0, "Coffee"),     // a subcategory
                expense(2, "2026-06-02 12:00:00", 6.0, "Groceries"),  // a subcategory
                expense(3, "2026-06-03 12:00:00", 20.0, "Transport"), // a top-level
                Txn { id: 4, ts: "2026-06-04 12:00:00".into(), kind: TxnKind::Income, amount: 999.0,
                      account_id: 1, to_account_id: None, category: "Salary".into(), note: String::new() },
            ],
            ..Model::default()
        };
        let b = category_breakdown(&model, TxnKind::Expense);
        // each logged category stands on its own (no roll-up); sorted by amount; income excluded
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], ("Transport".to_string(), 20.0));
        assert_eq!(b[1], ("Groceries".to_string(), 6.0));
        assert_eq!(b[2], ("Coffee".to_string(), 4.0));
    }

    #[test]
    fn currency_and_dates_follow_the_locale() {
        // device locale drives both the formatting locale and a sensible default currency
        let mut m = Model::default();
        assert_eq!(money(&m, 1234.5), "€1,234.50"); // EnUs / Eur defaults
        m.locale = Locale::UkUa;
        m.currency = default_currency(Locale::UkUa);
        assert_eq!(money(&m, 1234.5), "1\u{a0}234,50 ₴");
        assert_eq!(fmt_date(&m, "2026-12-31"), "31.12.2026");
        // an unparseable date falls back to the raw string
        assert_eq!(fmt_date(&m, "n/a"), "n/a");
    }

    #[test]
    fn language_override_beats_the_device_then_translates() {
        let mut m = Model { device_lang: "uk".into(), ..Model::default() };
        assert_eq!(lang(&m), "uk"); // follows the device by default
        assert_eq!(tr(&m, "action.save"), "Зберегти");
        m.lang_override = Some("de".into()); // manual choice wins
        assert_eq!(lang(&m), "de");
        assert_eq!(tr(&m, "action.save"), "Speichern");
        // a key with no translation for the language falls back to English (the catalog default)
        assert_eq!(tr(&m, "tab.bills"), "Buchungen");
    }

    #[test]
    fn catalog_covers_every_language_for_each_key() {
        // guard against a half-translated key slipping in: every entry must have all five languages
        let c = catalog();
        for key in [
            "tab.bills", "period.day", "income", "net", "bills.empty", "stats.trend",
            "field.amount", "kind.asset", "settings.language", "err.category",
            "settings.currency", "settings.categories", "categories.manage", "data.backup",
            "categories.top_level", "action.add", "action.done", "err.cat_name",
            "settings.security", "lock.desc", "lock.title", "lock.unlock", "lock.prompt",
            "settings.appearance", "appearance.light", "appearance.dark",
            "sheet.editaccount", "err.acct_in_use", "action.cancel", "err.dest", "err.twoaccounts",
            "categories.add", "sheet.newcategory", "sheet.editcategory",
        ] {
            for langs in SUPPORTED {
                assert_ne!(c.tr(key, langs), key, "missing {langs} translation for {key}");
            }
        }
    }

    #[test]
    fn default_currency_is_region_implied() {
        assert_eq!(default_currency(Locale::UkUa), Currency::Uah);
        assert_eq!(default_currency(Locale::DeCh), Currency::Chf);
        assert_eq!(default_currency(Locale::EnUs), Currency::Usd);
        assert_eq!(default_currency(Locale::DeDe), Currency::Eur);
    }

    #[test]
    fn monthly_totals_splits_income_and_expense_for_the_month() {
        let txns = vec![
            expense(1, "2026-06-01 12:00:00", 12.0, "Coffee"),
            Txn { id: 2, ts: "2026-06-02 12:00:00".into(), kind: TxnKind::Income, amount: 500.0,
                  account_id: 1, to_account_id: None, category: "Salary".into(), note: String::new() },
            expense(3, "2026-05-30 12:00:00", 99.0, "Coffee"), // different month, ignored
        ];
        let (inc, exp) = monthly_totals(&txns, "2026-06");
        assert!((inc - 500.0).abs() < 1e-9);
        assert!((exp - 12.0).abs() < 1e-9);
    }

    #[test]
    fn csv_fields_are_quoted_only_when_needed() {
        assert_eq!(csv_field("Coffee"), "Coffee");
        assert_eq!(csv_field("Lunch, tip"), "\"Lunch, tip\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    fn sample_model() -> Model {
        Model {
            accounts: vec![
                Account { id: 1, name: "Cash".into(), kind: AccountKind::Asset, opening: 100.0 },
                Account { id: 2, name: "Bank, EU".into(), kind: AccountKind::Asset, opening: 0.0 },
            ],
            categories: cats(),
            txns: vec![
                expense(1, "2026-06-01 12:00:00", 4.0, "Coffee"),
                Txn { id: 2, ts: "2026-06-02 12:00:00".into(), kind: TxnKind::Transfer, amount: 30.0,
                      account_id: 1, to_account_id: Some(2), category: String::new(), note: "rent".into() },
            ],
            recurring: vec![rule(9, Freq::Monthly, "2026-07-01", None)],
            ..Model::default()
        }
    }

    #[test]
    fn build_csv_has_a_header_and_quotes_embedded_commas() {
        let csv = build_csv(&sample_model());
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "date,type,amount,account,to_account,category,note");
        assert_eq!(lines.len(), 3); // header + 2 txns
        assert!(lines[1].starts_with("2026-06-01,expense,4.00,Cash,,Coffee"));
        assert!(lines[2].contains("\"Bank, EU\"")); // the transfer destination is quoted
    }

    #[test]
    fn backup_round_trips_through_restore_statements() {
        let json = build_backup(&sample_model());
        let b: Backup = serde_json::from_str(&json).unwrap();
        assert_eq!(b.accounts.len(), 2);
        assert_eq!(b.txns.len(), 2);
        assert_eq!(b.recurring.len(), 1);

        let stmts = restore_statements(&b);
        // 4 DELETEs + 2 accounts + 5 categories + 2 txns + 1 recurring = 14
        assert_eq!(stmts.iter().filter(|s| s.contains("DELETE FROM")).count(), 4);
        assert_eq!(stmts.len(), 14);
        // ids are preserved so account_id references survive
        assert!(stmts.iter().any(|s| s.contains("INSERT INTO account") && s.contains("\"1\"")));
    }
}
