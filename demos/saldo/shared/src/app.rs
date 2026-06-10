//! Saldo — a multilingual, SQLite-backed personal expense & money manager, built on Mobiler.
//!
//! Grows over the tutorial (`docs/tutorial/saldo/`). **Chapter 6** makes money and dates
//! locale-aware: it reads the device locale at startup (`cx.device_locale` → `Locale::from_tag`) and
//! formats every amount and date through `mobiler_core::format` (which gains **Ukrainian** —
//! `Locale::UkUa` / `Currency::Uah`). Earlier chapters set up the four-tab shell, `SQLite` persistence
//! with `cx.now()`, the accounts / transfers ledger, categories, the entry sheet, and the Stats charts.
//! Translating the UI strings (multilingual `i18n`), recurring and CSV export come next.

use mobiler_core::format::{format_currency, format_date};
use mobiler_core::{
    ButtonStyle, CardStyle, ChartSeries, ChartStyle, Currency, Cx, Icon, InputValue, Locale,
    MobilerApp, MobilerShell, Rgb, Segment, Spacing, Widget, button, caption, card, chart, chip,
    column, divider, donut_chart, emphasis, row, scaffold, segment, segmented, spacer, subtitle, text,
    text_field, with_fab, with_sheet,
};
use serde::{Deserialize, Serialize};

// ---- schema + migrations (run once, gated by PRAGMA user_version) ----

const SCHEMA_VERSION: u32 = 3;

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
    if from < SCHEMA_VERSION {
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
    q
}

const LOAD_ACCOUNTS: &str = "SELECT id, name, kind, opening, sort FROM account ORDER BY sort, id";
const LOAD_CATEGORIES: &str =
    "SELECT id, name, kind, parent_id, sort FROM category ORDER BY kind, sort, id";
const LOAD_TXNS: &str = "SELECT id, ts, kind, amount, account_id, to_account_id, category, note \
                         FROM txn ORDER BY ts DESC, id DESC";

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

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub id: u32,
    pub name: String,
    pub kind: AccountKind,
    pub opening: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub id: u32,
    pub name: String,
    pub kind: String, // "income" | "expense"
    pub parent_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Default)]
pub struct Model {
    screen: Screen,
    period: Period,
    stats_kind: TxnKind, // expense/income toggle on the Stats tab
    locale: Locale,      // formatting locale, from the device at startup
    currency: Currency,  // base currency (defaulted from the locale; user-set in Settings later)
    today: String,
    accounts: Vec<Account>,
    categories: Vec<Category>,
    txns: Vec<Txn>,
    pending_sql: Vec<String>,

    // "new transaction" sheet
    adding: bool,
    draft_kind: TxnKind,
    draft_amount: String,
    draft_account: Option<u32>,
    draft_to_account: Option<u32>,
    draft_category: String, // the chosen category *name*
    draft_date: String,     // "YYYY-MM-DD" (defaults to today; changeable via the date picker)
    draft_note: String,

    // "new account" sheet
    adding_account: bool,
    acc_name: String,
    acc_kind: AccountKind,
    acc_opening: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Switch(Screen),
    SetPeriod(Period),
    SetStatsKind(TxnKind),
    GotLocale(String),
    // schema / load
    Schema(String),
    Migrated,
    AccountsLoaded(String),
    CategoriesLoaded(String),
    Loaded(String),
    GotToday(String),
    Reload,
    // new transaction
    StartAdd,
    CancelAdd,
    SetKind(TxnKind),
    SetAccount(u32),
    SetToAccount(u32),
    SetCategory(String),
    PickDate,
    DatePicked(String),
    Save,
    Saved(bool),
    Delete(u32),
    // new account
    StartAddAccount,
    CancelAddAccount,
    SetAccKind(AccountKind),
    SaveAccount,
    AccountSaved(bool),
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
                    model.currency = default_currency(loc);
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
            Msg::GotToday(s) => {
                if let Some(d) = s.get(..10) {
                    model.today = d.to_string();
                }
            }

            Msg::StartAdd => {
                model.adding = true;
                model.draft_kind = TxnKind::Expense;
                model.draft_amount.clear();
                model.draft_category.clear();
                model.draft_note.clear();
                model.draft_date = model.today.clone();
                model.draft_account = model.accounts.first().map(|a| a.id);
                model.draft_to_account = model.accounts.get(1).map(|a| a.id);
            }
            Msg::CancelAdd => model.adding = false,
            Msg::SetKind(k) => {
                model.draft_kind = k;
                model.draft_category.clear(); // categories differ per kind
            }
            Msg::SetAccount(id) => model.draft_account = Some(id),
            Msg::SetToAccount(id) => model.draft_to_account = Some(id),
            Msg::SetCategory(name) => model.draft_category = name,
            Msg::PickDate => cx.pick_date(|r| Msg::DatePicked(if r.ok { r.output } else { String::new() })),
            Msg::DatePicked(date) => {
                if !date.is_empty() {
                    model.draft_date = date;
                }
            }
            Msg::Save => {
                if let Err(why) = validate_txn(model) {
                    cx.notify("toast", "show", why);
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
                let sql = serde_json::json!({
                    "sql": "INSERT INTO txn(ts, kind, amount, account_id, to_account_id, category, note) \
                            VALUES (?, ?, ?, ?, ?, ?, ?)",
                    "args": [ts, model.draft_kind.db(), amount.to_string(),
                             account_id.to_string(), to, category, model.draft_note.trim()],
                })
                .to_string();
                cx.plugin("sqlite", "exec", sql, |r| Msg::Saved(r.ok));
            }
            Msg::Saved(ok) => {
                if ok {
                    model.adding = false;
                    load_all(cx);
                } else {
                    cx.notify("toast", "show", "Could not save — please try again.");
                }
            }
            Msg::Delete(id) => {
                let sql = serde_json::json!({ "sql": "DELETE FROM txn WHERE id = ?", "args": [id.to_string()] })
                    .to_string();
                cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
            }

            Msg::StartAddAccount => {
                model.adding_account = true;
                model.acc_name.clear();
                model.acc_kind = AccountKind::Asset;
                model.acc_opening.clear();
            }
            Msg::CancelAddAccount => model.adding_account = false,
            Msg::SetAccKind(k) => model.acc_kind = k,
            Msg::SaveAccount => {
                if model.acc_name.trim().is_empty() {
                    cx.notify("toast", "show", "Give the account a name.");
                    return;
                }
                let opening = model.acc_opening.trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
                let sql = serde_json::json!({
                    "sql": "INSERT INTO account(name, kind, opening, sort) VALUES (?, ?, ?, 0)",
                    "args": [model.acc_name.trim(), model.acc_kind.db(), opening.to_string()],
                })
                .to_string();
                cx.plugin("sqlite", "exec", sql, |r| Msg::AccountSaved(r.ok));
            }
            Msg::AccountSaved(ok) => {
                if ok {
                    model.adding_account = false;
                    load_all(cx);
                }
            }
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        if let InputValue::Text(v) = value {
            match id {
                "amount" => model.draft_amount = v,
                "note" => model.draft_note = v,
                "acc_name" => model.acc_name = v,
                "acc_opening" => model.acc_opening = v,
                _ => {}
            }
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let tabs = vec![
            tab(model.screen, Screen::Bills, "Bills", Icon::Home),
            tab(model.screen, Screen::Stats, "Stats", Icon::Star),
            tab(model.screen, Screen::Assets, "Assets", Icon::Cart),
            tab(model.screen, Screen::Settings, "Settings", Icon::Settings),
        ];
        let (heading, body) = match model.screen {
            Screen::Bills => ("Saldo", bills(model)),
            Screen::Stats => ("Stats", stats(model)),
            Screen::Assets => ("Assets", assets(model)),
            Screen::Settings => ("Settings", soon("Currency, language & more arrive later.")),
        };

        let mut root = scaffold(heading, false, tabs, body);
        match model.screen {
            Screen::Bills => root = with_fab(root, Icon::Add, Msg::StartAdd),
            Screen::Assets => root = with_fab(root, Icon::Add, Msg::StartAddAccount),
            _ => {}
        }
        if model.adding {
            root = with_sheet(root, "New transaction", txn_sheet(model), Msg::CancelAdd);
        } else if model.adding_account {
            root = with_sheet(root, "New account", account_sheet(model), Msg::CancelAddAccount);
        }
        root
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
}

fn tab(current: Screen, screen: Screen, label: &str, icon: Icon) -> mobiler_core::Tab {
    mobiler_core::tab_icon(label, icon, current == screen, Msg::Switch(screen))
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

fn validate_txn(model: &Model) -> Result<(), &'static str> {
    if parse_amount(&model.draft_amount).is_none() {
        return Err("Enter an amount greater than zero.");
    }
    if model.draft_account.is_none() {
        return Err("Pick an account.");
    }
    if model.draft_kind == TxnKind::Transfer {
        match model.draft_to_account {
            None => return Err("Pick a destination account."),
            id if id == model.draft_account => return Err("Pick two different accounts."),
            _ => {}
        }
    } else if model.draft_category.trim().is_empty() {
        return Err("Pick a category.");
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

/// Resolve a leaf category name to its **top-level** parent name (a subcategory → its parent, a
/// top-level or unknown name → itself), so the Stats breakdown rolls subcategories up.
fn top_category<'a>(categories: &'a [Category], kind: &str, name: &'a str) -> &'a str {
    let Some(c) = categories.iter().find(|c| c.kind == kind && c.name == name) else {
        return name;
    };
    match c.parent_id {
        Some(pid) => categories.iter().find(|p| p.id == pid).map_or(name, |p| p.name.as_str()),
        None => name,
    }
}

/// In-period transactions of `kind`, summed by top-level category and sorted by amount descending.
fn category_breakdown(model: &Model, kind: TxnKind) -> Vec<(String, f64)> {
    let ck = kind.category_kind();
    let mut totals: Vec<(String, f64)> = Vec::new();
    for t in model.txns.iter().filter(|t| t.kind == kind && in_period(&t.ts, model.period, &model.today))
    {
        let name = top_category(&model.categories, ck, &t.category).to_string();
        match totals.iter_mut().find(|(n, _)| *n == name) {
            Some((_, amt)) => *amt += t.amount,
            None => totals.push((name, t.amount)),
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

// ---- view pieces ----

fn soon(msg: &str) -> Widget {
    column(vec![spacer(Spacing::Xl), caption(msg)])
}

fn period_seg(current: Period, p: Period, label: &str) -> Segment {
    segment(label, current == p, Msg::SetPeriod(p))
}

fn bills(model: &Model) -> Widget {
    let shown: Vec<&Txn> =
        model.txns.iter().filter(|t| in_period(&t.ts, model.period, &model.today)).collect();
    let (income, expense) = period_totals(&model.txns, model.period, &model.today);

    let header = card(
        column(vec![
            segmented(vec![
                period_seg(model.period, Period::Day, "Day"),
                period_seg(model.period, Period::Month, "Month"),
                period_seg(model.period, Period::All, "All"),
            ]),
            spacer(Spacing::Sm),
            row(vec![caption("Income"), spacer(Spacing::Md), emphasis(money(model, income))]),
            row(vec![caption("Expense"), spacer(Spacing::Md), emphasis(money(model, expense))]),
            divider(),
            row(vec![caption("Net"), spacer(Spacing::Md), emphasis(money(model, income - expense))]),
        ]),
        CardStyle::Filled,
    );

    if shown.is_empty() {
        return column(vec![
            header,
            spacer(Spacing::Xl),
            caption("Nothing here yet — tap + to add a transaction."),
        ]);
    }

    let mut sections = vec![header, spacer(Spacing::Md)];
    for (day, items) in group_by_day(&shown) {
        let mut rows = vec![row(vec![subtitle(fmt_date(model, day)), spacer(Spacing::Md)]), divider()];
        for t in items {
            rows.push(txn_row(model, t));
        }
        sections.push(card(column(rows), CardStyle::Elevated));
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
    row(vec![
        label,
        spacer(Spacing::Md),
        emphasis(signed(model, t)),
        button("Delete", ButtonStyle::Text, Msg::Delete(t.id)),
    ])
}

fn assets(model: &Model) -> Widget {
    let (assets_total, liabilities, net) = net_worth(&model.accounts, &model.txns);
    let summary = card(
        column(vec![
            row(vec![caption("Assets"), spacer(Spacing::Md), emphasis(money(model, assets_total))]),
            row(vec![caption("Liabilities"), spacer(Spacing::Md), emphasis(money(model, liabilities))]),
            divider(),
            row(vec![text("Net worth"), spacer(Spacing::Md), emphasis(money(model, net))]),
        ]),
        CardStyle::Filled,
    );

    if model.accounts.is_empty() {
        return column(vec![summary, spacer(Spacing::Xl), caption("Tap + to add an account.")]);
    }

    let mut rows = Vec::new();
    for a in &model.accounts {
        let tag = if a.kind == AccountKind::Liability { " (liability)" } else { "" };
        rows.push(row(vec![
            text(format!("{}{tag}", a.name)),
            spacer(Spacing::Md),
            emphasis(money(model, balance(a, &model.txns))),
        ]));
    }
    column(vec![summary, spacer(Spacing::Md), card(column(rows), CardStyle::Elevated)])
}

/// The Stats tab: a category-breakdown donut (income/expense toggle, current period) with a ranked
/// list, a net-worth trend line, and a monthly income-vs-expense bar chart — all from the `Chart` widget.
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)] // money f64 → chart f32 is fine here
fn stats(model: &Model) -> Widget {
    if model.txns.is_empty() {
        return column(vec![
            spacer(Spacing::Xl),
            caption("Add a few transactions and your charts appear here."),
        ]);
    }

    let kind = model.stats_kind;
    let controls = card(
        column(vec![
            segmented(vec![
                period_seg(model.period, Period::Day, "Day"),
                period_seg(model.period, Period::Month, "Month"),
                period_seg(model.period, Period::All, "All"),
            ]),
            spacer(Spacing::Sm),
            segmented(vec![
                segment("Expense", kind == TxnKind::Expense, Msg::SetStatsKind(TxnKind::Expense)),
                segment("Income", kind == TxnKind::Income, Msg::SetStatsKind(TxnKind::Income)),
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
                subtitle("By category"),
                spacer(Spacing::Sm),
                caption(if kind == TxnKind::Income {
                    "No income in this period."
                } else {
                    "No expenses in this period."
                }),
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
        let mut items = vec![subtitle("By category"), spacer(Spacing::Sm), donut, divider()];
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
            subtitle("Net-worth trend"),
            spacer(Spacing::Sm),
            chart(vec![ChartSeries::new("Net worth", trend)], labels.clone(), ChartStyle::Line, true, false),
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
            subtitle("Monthly income vs expense"),
            spacer(Spacing::Sm),
            chart(
                vec![
                    ChartSeries::new("Income", inc).with_color(palette(2)),
                    ChartSeries::new("Expense", exp).with_color(palette(1)),
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
    row(model.accounts.iter().map(|a| chip(a.name.clone(), selected == Some(a.id), on(a.id))).collect())
}

/// A two-level category picker: top-level chips, plus the subcategory row of whichever top-level the
/// current selection belongs to. Selecting either level stores that category's name.
fn category_picker(model: &Model) -> Widget {
    let kind = model.draft_kind.category_kind();
    let sel = &model.draft_category;
    let tops: Vec<Widget> = model
        .categories
        .iter()
        .filter(|c| c.kind == kind && c.parent_id.is_none())
        .map(|c| chip(c.name.clone(), &c.name == sel, Msg::SetCategory(c.name.clone())))
        .collect();

    let mut items = vec![caption("Category"), row(tops)];
    if let Some(pid) = open_parent(&model.categories, kind, sel) {
        let kids: Vec<Widget> = model
            .categories
            .iter()
            .filter(|c| c.kind == kind && c.parent_id == Some(pid))
            .map(|c| chip(c.name.clone(), &c.name == sel, Msg::SetCategory(c.name.clone())))
            .collect();
        if !kids.is_empty() {
            items.push(row(kids));
        }
    }
    column(items)
}

fn txn_sheet(model: &Model) -> Widget {
    let mut items = vec![
        segmented(vec![
            segment("Expense", model.draft_kind == TxnKind::Expense, Msg::SetKind(TxnKind::Expense)),
            segment("Income", model.draft_kind == TxnKind::Income, Msg::SetKind(TxnKind::Income)),
            segment("Transfer", model.draft_kind == TxnKind::Transfer, Msg::SetKind(TxnKind::Transfer)),
        ]),
        spacer(Spacing::Sm),
        text_field("amount", "Amount (e.g. 12.50)", model.draft_amount.clone()),
        row(vec![
            caption("Date"),
            spacer(Spacing::Md),
            text(fmt_date(model, &model.draft_date)),
            button("Change", ButtonStyle::Text, Msg::PickDate),
        ]),
        caption(if model.draft_kind == TxnKind::Transfer { "From account" } else { "Account" }),
        account_chips(model, model.draft_account, Msg::SetAccount),
    ];
    if model.draft_kind == TxnKind::Transfer {
        items.push(caption("To account"));
        items.push(account_chips(model, model.draft_to_account, Msg::SetToAccount));
    } else {
        items.push(category_picker(model));
    }
    items.push(text_field("note", "Note (optional)", model.draft_note.clone()));
    items.push(spacer(Spacing::Md));
    items.push(button("Save", ButtonStyle::Filled, Msg::Save));
    column(items)
}

fn account_sheet(model: &Model) -> Widget {
    column(vec![
        text_field("acc_name", "Account name (e.g. Cash, Bank)", model.acc_name.clone()),
        segmented(vec![
            segment("Asset", model.acc_kind == AccountKind::Asset, Msg::SetAccKind(AccountKind::Asset)),
            segment("Liability", model.acc_kind == AccountKind::Liability, Msg::SetAccKind(AccountKind::Liability)),
        ]),
        text_field("acc_opening", "Opening balance (optional)", model.acc_opening.clone()),
        spacer(Spacing::Md),
        button("Save", ButtonStyle::Filled, Msg::SaveAccount),
    ])
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
    fn migration_targets_version_3() {
        assert_eq!(*migrations_from(0).last().unwrap(), "PRAGMA user_version = 3");
        // a v2 device only needs the v3 (category) block
        let from2 = migrations_from(2);
        assert_eq!(*from2.last().unwrap(), "PRAGMA user_version = 3");
        assert!(from2.iter().all(|s| !s.contains("CREATE TABLE IF NOT EXISTS txn")));
        assert!(migrations_from(3).is_empty());
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
    fn income_expense_need_a_category_transfer_does_not() {
        let mut m = Model { draft_amount: "5".into(), ..Model::default() };
        m.draft_account = Some(1);
        // expense with no category → rejected
        assert!(validate_txn(&m).is_err());
        m.draft_category = "Coffee".into();
        assert!(validate_txn(&m).is_ok());
        // transfer ignores category but needs a distinct destination
        m.draft_kind = TxnKind::Transfer;
        m.draft_category.clear();
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
    fn category_breakdown_rolls_up_subcategories_and_sorts() {
        let model = Model {
            categories: cats(),
            today: "2026-06-10".into(),
            period: Period::All,
            txns: vec![
                expense(1, "2026-06-01 12:00:00", 4.0, "Coffee"),     // → Food & Drink
                expense(2, "2026-06-02 12:00:00", 6.0, "Groceries"),  // → Food & Drink
                expense(3, "2026-06-03 12:00:00", 20.0, "Transport"), // top-level
                Txn { id: 4, ts: "2026-06-04 12:00:00".into(), kind: TxnKind::Income, amount: 999.0,
                      account_id: 1, to_account_id: None, category: "Salary".into(), note: String::new() },
            ],
            ..Model::default()
        };
        let b = category_breakdown(&model, TxnKind::Expense);
        // Transport (20) ranks above the rolled-up Food & Drink (10); income excluded
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].0, "Transport");
        assert!((b[0].1 - 20.0).abs() < 1e-9);
        assert_eq!(b[1].0, "Food & Drink");
        assert!((b[1].1 - 10.0).abs() < 1e-9);
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
}
