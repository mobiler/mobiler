//! Saldo — a multilingual, SQLite-backed personal expense & money manager, built on Mobiler.
//!
//! This file grows over the tutorial (`docs/tutorial/saldo/`). **Chapter 2** makes the ledger
//! durable: expenses live in on-device `SQLite` (via the bundled `sqlite` plugin), are stamped with
//! the current date-time (the new `cx.now()` capability), grouped by day, and filtered by period.
//! Accounts, categories, charts, multilingual UI, recurring entries and CSV export arrive later.

use mobiler_core::{
    ButtonStyle, CardStyle, Cx, Icon, InputValue, MobilerApp, MobilerShell, Segment, Spacing, Widget,
    button, caption, card, column, divider, emphasis, row, scaffold, segment, segmented, spacer,
    subtitle, text, text_field, with_fab, with_sheet,
};
use serde::{Deserialize, Serialize};

/// One fixed table. The app owns its schema — `sqlite` just runs SQL. `IF NOT EXISTS` makes
/// running this on every launch idempotent (our cheap "migration").
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS expense(\
    id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, amount TEXT NOT NULL, \
    category TEXT NOT NULL, note TEXT NOT NULL DEFAULT '')";

const LOAD_SQL: &str = "SELECT id, ts, amount, category, note FROM expense ORDER BY ts DESC";

/// Which of the four tabs is showing.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Screen {
    #[default]
    Bills,
    Stats,
    Assets,
    Settings,
}

/// The Bills period filter.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Period {
    Day,
    #[default]
    Month,
    All,
}

/// One ledger entry, as loaded from `SQLite`.
#[derive(Clone, Debug, PartialEq)]
pub struct Expense {
    pub id: u32,
    pub ts: String, // local "YYYY-MM-DD HH:MM:SS" — sortable, and its first 10 chars are the day
    pub amount: f64,
    pub category: String,
    pub note: String,
}

impl Expense {
    fn day(&self) -> &str {
        self.ts.get(..10).unwrap_or(&self.ts)
    }
}

#[derive(Default)]
pub struct Model {
    screen: Screen,
    period: Period,
    today: String, // "YYYY-MM-DD" from cx.now() at startup — anchors the Day/Month filters
    expenses: Vec<Expense>,
    // The "new expense" sheet: whether it's open + its draft fields.
    adding: bool,
    draft_amount: String,
    draft_category: String,
    draft_note: String,
}

/// The app's typed events. Several are *responses* from async capabilities (`SQLite`, the clock).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Switch(Screen),
    SetPeriod(Period),
    StartAdd,
    CancelAdd,
    Save,
    Stamped(String), // cx.now() replied → perform the INSERT with this timestamp
    Saved(bool),     // the INSERT/DELETE finished → reload
    Delete(u32),
    Reload,
    Loaded(String),  // JSON rows from the SELECT
    GotToday(String),
}

#[derive(Default)]
pub struct SaldoApp;

impl MobilerApp for SaldoApp {
    type Event = Msg;
    type Model = Model;

    fn init(&self, _model: &mut Model, cx: &mut Cx<Msg>) {
        // Create the table if needed, then load. The data lives in SQLite; the Model doesn't,
        // so we re-read it on every launch. Also stamp "today" for the period filter.
        cx.plugin("sqlite", "exec", SCHEMA, |_| Msg::Reload);
        cx.now(|r| Msg::GotToday(r.output));
    }

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Switch(s) => model.screen = s,
            Msg::SetPeriod(p) => model.period = p,
            Msg::StartAdd => {
                model.adding = true;
                model.draft_amount.clear();
                model.draft_category.clear();
                model.draft_note.clear();
            }
            Msg::CancelAdd => model.adding = false,
            Msg::Save => {
                // Validate in the core. Only ask the clock for "now" once the input is good.
                if parse_amount(&model.draft_amount).is_none() {
                    cx.notify("toast", "show", "Enter an amount greater than zero.");
                    return;
                }
                cx.now(|r| Msg::Stamped(if r.ok { r.output } else { String::new() }));
            }
            Msg::Stamped(ts) => {
                let (Some(amount), false) = (parse_amount(&model.draft_amount), ts.is_empty()) else {
                    return;
                };
                let category = match model.draft_category.trim() {
                    "" => "Uncategorized",
                    c => c,
                };
                // Bound parameters (`args`) — never string-concatenate user text into SQL.
                let sql = serde_json::json!({
                    "sql": "INSERT INTO expense(ts, amount, category, note) VALUES (?, ?, ?, ?)",
                    "args": [ts, amount.to_string(), category, model.draft_note.trim()],
                })
                .to_string();
                cx.plugin("sqlite", "exec", sql, |r| Msg::Saved(r.ok));
            }
            Msg::Saved(ok) => {
                if ok {
                    model.adding = false;
                    load(cx);
                } else {
                    cx.notify("toast", "show", "Could not save — please try again.");
                }
            }
            Msg::Delete(id) => {
                let sql = serde_json::json!({
                    "sql": "DELETE FROM expense WHERE id = ?",
                    "args": [id.to_string()],
                })
                .to_string();
                cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
            }
            Msg::Reload => load(cx),
            Msg::Loaded(json) => model.expenses = parse_rows(&json),
            Msg::GotToday(s) => {
                if let Some(date) = s.get(..10) {
                    model.today = date.to_string();
                }
            }
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        if let InputValue::Text(v) = value {
            match id {
                "amount" => model.draft_amount = v,
                "category" => model.draft_category = v,
                "note" => model.draft_note = v,
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
            Screen::Stats => ("Stats", soon("Charts arrive in Chapter 5.")),
            Screen::Assets => ("Assets", soon("Accounts & net worth arrive in Chapter 3.")),
            Screen::Settings => ("Settings", soon("Currency, language & more arrive later.")),
        };

        let mut root = scaffold(heading, false, tabs, body);
        if model.screen == Screen::Bills {
            root = with_fab(root, Icon::Add, Msg::StartAdd);
        }
        if model.adding {
            root = with_sheet(root, "New expense", add_sheet(model), Msg::CancelAdd);
        }
        root
    }
}

/// Re-read the ledger from `SQLite` into the model (the `Model` isn't persisted — `SQLite` is).
fn load(cx: &mut Cx<Msg>) {
    cx.plugin("sqlite", "query", LOAD_SQL, |r| {
        Msg::Loaded(if r.ok { r.output } else { "[]".to_string() })
    });
}

fn tab(current: Screen, screen: Screen, label: &str, icon: Icon) -> mobiler_core::Tab {
    mobiler_core::tab_icon(label, icon, current == screen, Msg::Switch(screen))
}

// ---- pure helpers (unit-tested below) ----

/// Parse a user-typed amount, accepting both `12.50` and `12,50`. `None` if not a positive number.
fn parse_amount(s: &str) -> Option<f64> {
    let v = s.trim().replace(',', ".").parse::<f64>().ok()?;
    (v > 0.0).then_some(v)
}

/// Parse the `sqlite` `query` result — a JSON array of string-keyed **string** rows.
fn parse_rows(json: &str) -> Vec<Expense> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            Some(Expense {
                id: r.get("id")?.as_str()?.parse().ok()?,
                ts: r.get("ts")?.as_str()?.to_string(),
                amount: r.get("amount")?.as_str()?.parse().ok()?,
                category: r.get("category")?.as_str()?.to_string(),
                note: r.get("note").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn in_period(e: &Expense, period: Period, today: &str) -> bool {
    match period {
        Period::All => true,
        Period::Day => e.day() == today,
        Period::Month => !today.is_empty() && e.ts.get(..7) == today.get(..7),
    }
}

/// Group an already-newest-first slice into consecutive same-day sections.
fn group_by_day<'a>(items: &[&'a Expense]) -> Vec<(&'a str, Vec<&'a Expense>)> {
    let mut out: Vec<(&str, Vec<&Expense>)> = Vec::new();
    for &e in items {
        match out.last_mut() {
            Some((day, group)) if *day == e.day() => group.push(e),
            _ => out.push((e.day(), vec![e])),
        }
    }
    out
}

/// Plain `€00.00` for now; locale-aware currency formatting lands in Chapter 6.
fn money(v: f64) -> String {
    format!("€{v:.2}")
}

// ---- view pieces ----

fn soon(msg: &str) -> Widget {
    column(vec![spacer(Spacing::Xl), caption(msg)])
}

fn period_seg(current: Period, p: Period, label: &str) -> Segment {
    segment(label, current == p, Msg::SetPeriod(p))
}

fn bills(model: &Model) -> Widget {
    let shown: Vec<&Expense> =
        model.expenses.iter().filter(|e| in_period(e, model.period, &model.today)).collect();
    let total: f64 = shown.iter().map(|e| e.amount).sum();

    let header = card(
        column(vec![
            segmented(vec![
                period_seg(model.period, Period::Day, "Day"),
                period_seg(model.period, Period::Month, "Month"),
                period_seg(model.period, Period::All, "All"),
            ]),
            spacer(Spacing::Sm),
            row(vec![caption("Spent"), spacer(Spacing::Md), emphasis(money(total))]),
        ]),
        CardStyle::Filled,
    );

    if shown.is_empty() {
        return column(vec![
            header,
            spacer(Spacing::Xl),
            caption("Nothing here yet — tap + to add an expense."),
        ]);
    }

    let mut sections = vec![header, spacer(Spacing::Md)];
    for (day, items) in group_by_day(&shown) {
        let subtotal: f64 = items.iter().map(|e| e.amount).sum();
        let mut rows = vec![
            row(vec![subtitle(day), spacer(Spacing::Md), text(money(subtotal))]),
            divider(),
        ];
        for e in items {
            rows.push(entry_row(e));
        }
        sections.push(card(column(rows), CardStyle::Elevated));
        sections.push(spacer(Spacing::Sm));
    }
    column(sections)
}

fn entry_row(e: &Expense) -> Widget {
    let label = if e.note.is_empty() {
        column(vec![text(e.category.clone())])
    } else {
        column(vec![text(e.category.clone()), caption(e.note.clone())])
    };
    row(vec![
        label,
        spacer(Spacing::Md),
        emphasis(money(e.amount)),
        button("Delete", ButtonStyle::Text, Msg::Delete(e.id)),
    ])
}

fn add_sheet(model: &Model) -> Widget {
    column(vec![
        text_field("amount", "Amount (e.g. 12.50)", model.draft_amount.clone()),
        text_field("category", "Category (e.g. Groceries)", model.draft_category.clone()),
        text_field("note", "Note (optional)", model.draft_note.clone()),
        spacer(Spacing::Md),
        button("Save", ButtonStyle::Filled, Msg::Save),
    ])
}

/// The Crux app the FFI + codegen target — `MobilerShell` over `SaldoApp`.
pub type App = MobilerShell<SaldoApp>;

#[cfg(test)]
mod test {
    use super::*;

    const ROWS: &str = r#"[
        {"id":"3","ts":"2026-06-09 09:30:00","amount":"12.5","category":"Groceries","note":"milk"},
        {"id":"2","ts":"2026-06-09 08:00:00","amount":"3","category":"Coffee","note":""},
        {"id":"1","ts":"2026-06-08 19:00:00","amount":"40","category":"Dinner","note":""}
    ]"#;

    #[test]
    fn parses_string_rows_into_expenses() {
        let xs = parse_rows(ROWS);
        assert_eq!(xs.len(), 3);
        assert_eq!(xs[0].id, 3);
        assert!((xs[0].amount - 12.5).abs() < f64::EPSILON);
        assert_eq!(xs[0].category, "Groceries");
        assert_eq!(xs[1].note, "");
    }

    #[test]
    fn bad_rows_are_skipped_not_panicked() {
        assert!(parse_rows("not json").is_empty());
        assert!(parse_rows(r#"[{"id":"x"}]"#).is_empty()); // unparseable id → dropped
    }

    #[test]
    fn period_filter() {
        let xs = parse_rows(ROWS);
        let today = "2026-06-09";
        let count = |p: Period| xs.iter().filter(|e| in_period(e, p, today)).count();
        assert_eq!(count(Period::Day), 2); // two entries on the 9th
        assert_eq!(count(Period::Month), 3); // all in June 2026
        assert_eq!(count(Period::All), 3);
    }

    #[test]
    fn groups_consecutive_days() {
        let xs = parse_rows(ROWS);
        let refs: Vec<&Expense> = xs.iter().collect();
        let groups = group_by_day(&refs);
        assert_eq!(groups.len(), 2); // the 9th and the 8th
        assert_eq!(groups[0].0, "2026-06-09");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "2026-06-08");
    }

    #[test]
    fn parse_amount_accepts_comma_and_rejects_nonpositive() {
        assert_eq!(parse_amount("12,50"), Some(12.5));
        assert_eq!(parse_amount(" 4 "), Some(4.0));
        assert_eq!(parse_amount("0"), None);
        assert_eq!(parse_amount("-3"), None);
        assert_eq!(parse_amount("abc"), None);
    }

    #[test]
    fn loaded_populates_model_and_today_is_trimmed_to_date() {
        let app = SaldoApp;
        let mut model = Model::default();
        app.update(Msg::Loaded(ROWS.to_string()), &mut model, &mut Cx::default());
        assert_eq!(model.expenses.len(), 3);
        app.update(Msg::GotToday("2026-06-09 21:15:00".into()), &mut model, &mut Cx::default());
        assert_eq!(model.today, "2026-06-09");
    }

    #[test]
    fn invalid_save_does_not_proceed() {
        let app = SaldoApp;
        let mut model = Model::default();
        app.update(Msg::StartAdd, &mut model, &mut Cx::default());
        app.input("amount", InputValue::Text("0".into()), &mut model, &mut Cx::default());
        app.update(Msg::Save, &mut model, &mut Cx::default());
        assert!(model.adding, "sheet stays open on invalid input");
    }
}
