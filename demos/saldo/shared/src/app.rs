//! Saldo — a multilingual, SQLite-backed personal expense & money manager, built on Mobiler.
//!
//! This file grows over the tutorial (`docs/tutorial/saldo/`). **Chapter 1** is the skeleton:
//! the four-tab shell (Bills · Stats · Assets · Settings) and an in-memory expense ledger you
//! can add to and delete from. Persistence (`SQLite`), accounts, categories, charts, multilingual
//! UI, recurring entries and CSV export all land in later chapters.

use mobiler_core::{
    ButtonStyle, CardStyle, Cx, Icon, InputValue, MobilerApp, MobilerShell, Spacing, Widget,
    button, caption, card, column, divider, emphasis, row, scaffold, spacer, subtitle, text,
    text_field, with_fab, with_sheet,
};
use serde::{Deserialize, Serialize};

/// Which of the four tabs is showing.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Screen {
    #[default]
    Bills,
    Stats,
    Assets,
    Settings,
}

/// One ledger entry. In-memory for now — Chapter 2 moves it into `SQLite`.
#[derive(Clone, Debug)]
pub struct Expense {
    pub id: u32,
    pub amount: f64,
    pub category: String,
    pub note: String,
}

#[derive(Default)]
pub struct Model {
    screen: Screen,
    expenses: Vec<Expense>,
    next_id: u32,
    // The "new expense" sheet: whether it's open + its draft fields.
    adding: bool,
    draft_amount: String,
    draft_category: String,
    draft_note: String,
}

/// The app's typed events. Mobiler serializes these into opaque tokens; the native shell
/// never sees this type.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Switch(Screen),
    StartAdd,
    CancelAdd,
    Save,
    Delete(u32),
}

#[derive(Default)]
pub struct SaldoApp;

impl MobilerApp for SaldoApp {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Switch(s) => model.screen = s,
            Msg::StartAdd => {
                model.adding = true;
                model.draft_amount.clear();
                model.draft_category.clear();
                model.draft_note.clear();
            }
            Msg::CancelAdd => model.adding = false,
            Msg::Save => {
                // Accept both "12.50" and "12,50" (comma decimals are common in our locales).
                let amount = model.draft_amount.trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
                if amount <= 0.0 {
                    cx.notify("toast", "show", "Enter an amount greater than zero.");
                    return;
                }
                let category = match model.draft_category.trim() {
                    "" => "Uncategorized".to_string(),
                    c => c.to_string(),
                };
                model.next_id += 1;
                model.expenses.insert(
                    0,
                    Expense { id: model.next_id, amount, category, note: model.draft_note.trim().to_string() },
                );
                model.adding = false;
            }
            Msg::Delete(id) => model.expenses.retain(|e| e.id != id),
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

fn tab(current: Screen, screen: Screen, label: &str, icon: Icon) -> mobiler_core::Tab {
    mobiler_core::tab_icon(label, icon, current == screen, Msg::Switch(screen))
}

/// Plain `€00.00` for now; locale-aware currency formatting lands in Chapter 6.
fn money(v: f64) -> String {
    format!("€{v:.2}")
}

fn soon(msg: &str) -> Widget {
    column(vec![spacer(Spacing::Xl), caption(msg)])
}

fn bills(model: &Model) -> Widget {
    let total: f64 = model.expenses.iter().map(|e| e.amount).sum();
    let summary = card(
        row(vec![caption("Spent"), spacer(Spacing::Md), emphasis(money(total))]),
        CardStyle::Filled,
    );

    if model.expenses.is_empty() {
        return column(vec![
            summary,
            spacer(Spacing::Xl),
            caption("No expenses yet — tap + to add your first one."),
        ]);
    }

    // Chapter 1 keeps every entry under a single "Today" group; real dates and multi-day
    // grouping arrive with SQLite in Chapter 2.
    let mut rows = vec![
        row(vec![subtitle("Today"), spacer(Spacing::Md), text(money(total))]),
        divider(),
    ];
    for e in &model.expenses {
        let label = if e.note.is_empty() {
            column(vec![text(e.category.clone())])
        } else {
            column(vec![text(e.category.clone()), caption(e.note.clone())])
        };
        rows.push(row(vec![
            label,
            spacer(Spacing::Md),
            emphasis(money(e.amount)),
            button("Delete", ButtonStyle::Text, Msg::Delete(e.id)),
        ]));
    }

    column(vec![summary, spacer(Spacing::Md), card(column(rows), CardStyle::Elevated)])
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

/// The Crux app the FFI + codegen target — `MobilerShell` over `SaldoApp`, so the native
/// shell stays generic and is built once.
pub type App = MobilerShell<SaldoApp>;

#[cfg(test)]
mod test {
    use super::*;

    fn drafted(amount: &str, category: &str) -> (SaldoApp, Model) {
        let app = SaldoApp;
        let mut model = Model::default();
        app.update(Msg::StartAdd, &mut model, &mut Cx::default());
        app.input("amount", InputValue::Text(amount.into()), &mut model, &mut Cx::default());
        app.input("category", InputValue::Text(category.into()), &mut model, &mut Cx::default());
        (app, model)
    }

    #[test]
    fn save_adds_an_expense() {
        let (app, mut model) = drafted("12,50", "Groceries");
        app.update(Msg::Save, &mut model, &mut Cx::default());
        assert_eq!(model.expenses.len(), 1);
        assert!((model.expenses[0].amount - 12.5).abs() < f64::EPSILON);
        assert_eq!(model.expenses[0].category, "Groceries");
        assert!(!model.adding, "sheet closes after save");
    }

    #[test]
    fn zero_or_invalid_amount_is_rejected() {
        let (app, mut model) = drafted("0", "Food");
        app.update(Msg::Save, &mut model, &mut Cx::default());
        assert!(model.expenses.is_empty());
        assert!(model.adding, "sheet stays open so the user can fix it");
    }

    #[test]
    fn empty_category_falls_back() {
        let (app, mut model) = drafted("5", "");
        app.update(Msg::Save, &mut model, &mut Cx::default());
        assert_eq!(model.expenses[0].category, "Uncategorized");
    }

    #[test]
    fn delete_removes_by_id() {
        let (app, mut model) = drafted("5", "A");
        app.update(Msg::Save, &mut model, &mut Cx::default());
        let id = model.expenses[0].id;
        app.update(Msg::Delete(id), &mut model, &mut Cx::default());
        assert!(model.expenses.is_empty());
    }

    #[test]
    fn switch_changes_screen() {
        let app = SaldoApp;
        let mut model = Model::default();
        app.update(Msg::Switch(Screen::Stats), &mut model, &mut Cx::default());
        assert_eq!(model.screen, Screen::Stats);
    }
}
