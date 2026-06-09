# Chapter 1 — Scaffold & first screen

We start from nothing and end with a running, four-tab app whose **Bills** tab lets you add and delete
expenses. No database yet — entries live in memory — but the whole shape of the app is here: the
navigation shell, a floating action button, a bottom sheet for input, and a typed `update`/`view` loop.

## 1. Scaffold the app

Mobiler's CLI generates a **mobile-only** project — a Rust core plus native iOS and Android shells, no
web target:

```sh
mobiler new saldo --package rs.mobiler.saldo
```

You get:

```
saldo/
├── Cargo.toml            # workspace: members = ["shared"]
├── shared/               # the Rust app core + the uniffi/codegen bridge
│   ├── Cargo.toml        # depends on mobiler-core
│   └── src/
│       ├── app.rs        # ← you live here: Model / Msg / update / view
│       ├── lib.rs        # re-exports + uniffi scaffolding (generated, leave it)
│       ├── ffi.rs        # the FFI bridge (generated, leave it)
│       └── bin/codegen.rs# emits the Swift/Kotlin ABI types at build time
├── iOS/                  # SwiftUI shell (XcodeGen project + Render.swift)
└── Android/              # Jetpack Compose shell (Gradle + MainActivity.kt)
```

Everything you write for Saldo is in **`shared/src/app.rs`**. The native shells are generic — they
render whatever `Widget` tree your `view` returns — so you almost never touch Swift or Kotlin.

> In this repo Saldo is a workspace demo, so `shared/Cargo.toml` points `mobiler-core` at the in-repo
> crate (`path = "../../../mobiler-core"`); a standalone `mobiler new` app pins the published version
> instead (`mobiler-core = "0.30"`).

## 2. The shape of a Mobiler app

A Mobiler app is a tiny state machine (it's [Crux](https://redbadger.github.io/crux/) under the hood):

- a **`Model`** — your state,
- a **`Msg`** enum — everything that can happen,
- **`update`** — applies a `Msg` to the `Model` (and asks the shell to do side-effects via `cx`),
- **`input`** — receives text-field / toggle changes from the shell,
- **`view`** — turns the `Model` into a `Widget` tree.

### State

```rust
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Screen { #[default] Bills, Stats, Assets, Settings }

#[derive(Clone, Debug)]
pub struct Expense { pub id: u32, pub amount: f64, pub category: String, pub note: String }

#[derive(Default)]
pub struct Model {
    screen: Screen,
    expenses: Vec<Expense>,
    next_id: u32,
    // the "new expense" sheet: open? + its draft fields
    adding: bool,
    draft_amount: String,
    draft_category: String,
    draft_note: String,
}
```

### Events

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg { Switch(Screen), StartAdd, CancelAdd, Save, Delete(u32) }
```

`Msg` is `Serialize` because Mobiler turns each event into an opaque token it hands to the native shell;
when a button is tapped, that token comes back and is decoded into your `Msg`. The shell never knows
what `Msg` *is* — that's what keeps one native binary able to run any Mobiler app.

### `update` — the logic

```rust
fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
    match event {
        Msg::Switch(s) => model.screen = s,
        Msg::StartAdd => { model.adding = true; /* clear drafts */ }
        Msg::CancelAdd => model.adding = false,
        Msg::Save => {
            // accept "12.50" and "12,50" — comma decimals are common in our locales
            let amount = model.draft_amount.trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
            if amount <= 0.0 {
                cx.notify("toast", "show", "Enter an amount greater than zero.");
                return;
            }
            model.next_id += 1;
            model.expenses.insert(0, Expense { id: model.next_id, amount, /* … */ });
            model.adding = false;
        }
        Msg::Delete(id) => model.expenses.retain(|e| e.id != id),
    }
}
```

Two things worth noting:

- **`cx.notify("toast", "show", …)`** is a fire-and-forget capability call — the shell shows a native
  toast. (Capabilities that return data, like SQLite or the device locale, take a callback; we'll meet
  those in later chapters.)
- Validation lives in the core, in Rust — not in the UI. The sheet only closes when the entry is valid.

### `input` — text fields report here

Text fields, toggles and the like don't fire a `Msg`; they stream their value into `input` keyed by the
id you gave the widget:

```rust
fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
    if let InputValue::Text(v) = value {
        match id {
            "amount"   => model.draft_amount = v,
            "category" => model.draft_category = v,
            "note"     => model.draft_note = v,
            _ => {}
        }
    }
}
```

## 3. The view — a four-tab shell

`scaffold(title, dark_mode, tabs, body)` is the app frame: a title bar and a bottom tab bar. We build
the four tabs, pick a body per screen, then layer a **floating action button** (only on Bills) and the
**add sheet** (only when `adding`) on top with the `with_fab` / `with_sheet` combinators:

```rust
fn view(&self, model: &Model) -> Widget {
    let tabs = vec![
        tab(model.screen, Screen::Bills,    "Bills",    Icon::Home),
        tab(model.screen, Screen::Stats,    "Stats",    Icon::Star),
        tab(model.screen, Screen::Assets,   "Assets",   Icon::Cart),
        tab(model.screen, Screen::Settings, "Settings", Icon::Settings),
    ];
    let (heading, body) = match model.screen {
        Screen::Bills => ("Saldo", bills(model)),
        Screen::Stats => ("Stats", soon("Charts arrive in Chapter 5.")),
        // …
    };

    let mut root = scaffold(heading, false, tabs, body);
    if model.screen == Screen::Bills { root = with_fab(root, Icon::Add, Msg::StartAdd); }
    if model.adding { root = with_sheet(root, "New expense", add_sheet(model), Msg::CancelAdd); }
    root
}
```

`with_fab` and `with_sheet` are the framework's "set-a-field-or-no-op" combinators: they only attach to
a `Scaffold`, so you compose them by feeding the scaffold through them. Drive them from the model —
the sheet is open precisely when `model.adding` is true, and tapping outside it sends `Msg::CancelAdd`.

The Bills body is an ordinary list built from the model — a summary card plus a card of rows:

```rust
fn bills(model: &Model) -> Widget {
    let total: f64 = model.expenses.iter().map(|e| e.amount).sum();
    let summary = card(row(vec![caption("Spent"), spacer(Spacing::Md), emphasis(money(total))]),
                       CardStyle::Filled);
    if model.expenses.is_empty() {
        return column(vec![summary, spacer(Spacing::Xl),
                           caption("No expenses yet — tap + to add your first one.")]);
    }
    // …one row per expense: category + note on the left, amount + a Delete button on the right…
}
```

The add sheet is just more widgets — three text fields and a Save button:

```rust
fn add_sheet(model: &Model) -> Widget {
    column(vec![
        text_field("amount",   "Amount (e.g. 12.50)",     model.draft_amount.clone()),
        text_field("category", "Category (e.g. Groceries)", model.draft_category.clone()),
        text_field("note",     "Note (optional)",          model.draft_note.clone()),
        spacer(Spacing::Md),
        button("Save", ButtonStyle::Filled, Msg::Save),
    ])
}
```

The text fields' ids (`"amount"`, `"category"`, `"note"`) are exactly the ids `input` matches on — that
string is the whole contract between a field and your state.

## 4. Run it

```sh
cargo test                 # the core is plain Rust — unit-test the logic with no device
mobiler build android      # → an installable APK
# iOS: open iOS/ in Xcode (or the CI 'iOS build' lane) and run
```

Because the core is pure Rust, the logic is unit-testable without a simulator:

```rust
#[test]
fn save_adds_an_expense() {
    let (app, mut model) = drafted("12,50", "Groceries");
    app.update(Msg::Save, &mut model, &mut Cx::default());
    assert_eq!(model.expenses.len(), 1);
    assert!(!model.adding); // sheet closes after a valid save
}
```

## What we built

A real, navigable app: four tabs, a FAB, a modal sheet, validated input, and a live list — all from one
`view` function over a typed `Model`, with zero Swift or Kotlin. The catch: quit the app and your
expenses are gone, because the `Model` lives only in memory.

**Next:** [Chapter 2 — Persist with SQLite](02-persist-with-sqlite.md), where Saldo gets a real database.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
