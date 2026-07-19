# Chapter 3 — Accounts & transfers

So far Saldo only tracked expenses. A real money manager tracks **where the money is** — across
accounts (cash, bank, a credit card) — and supports three kinds of entry: **expense**, **income**, and
**transfer** between accounts. This chapter generalizes the ledger, adds an Assets tab with balances and
net worth, and — because we already shipped a `expense` table in Chapter 2 — introduces a real **schema
migration**.

## 1. Evolving the schema with `PRAGMA user_version`

SQLite gives every database a free integer you can use as a schema version: `PRAGMA user_version`. We
read it on launch, run whatever migrations are missing, and bump it — so migrations run **exactly once**
per device, even though `init()` runs on every launch.

```rust
const SCHEMA_VERSION: u32 = 2;

fn migrations_from(from: u32) -> Vec<&'static str> {
    let mut q = Vec::new();
    if from < SCHEMA_VERSION {
        q.extend_from_slice(&[
            "CREATE TABLE IF NOT EXISTS account(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, \
             kind TEXT NOT NULL DEFAULT 'asset', opening TEXT NOT NULL DEFAULT '0', sort INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS txn(id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, \
             kind TEXT NOT NULL, amount TEXT NOT NULL, account_id INTEGER NOT NULL, to_account_id INTEGER, \
             category TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT '')",
            "INSERT INTO account(name, kind, opening, sort) SELECT 'Cash','asset','0',0 \
             WHERE NOT EXISTS (SELECT 1 FROM account)",                       // seed a first account
            "CREATE TABLE IF NOT EXISTS expense(...)",                        // ensure the old table exists
            "INSERT INTO txn(ts, kind, amount, account_id, category, note) \
             SELECT ts,'expense',amount,(SELECT MIN(id) FROM account),category,note FROM expense", // carry data forward
            "DROP TABLE expense",
            "PRAGMA user_version = 2",
        ]);
    }
    q
}
```

This one list handles **both** a fresh install (the old `expense` table is created empty, copies nothing,
gets dropped) and a Chapter-2 device (its expenses are copied into `txn` as expense-type rows on the
seeded *Cash* account). The `user_version` bump at the end is what stops it re-running.

### Running the queue in order

Capability calls resolve asynchronously and independently, so to run migrations **in order** we drain a
small queue one statement at a time — each `exec` completion triggers the next:

```rust
fn init(&self, _model, cx) {
    cx.plugin("sqlite", "query", "PRAGMA user_version", |r| {
        Msg::Schema(if r.ok { r.as_text().unwrap_or_default().to_string() } else { "[]".to_string() })
    });
    cx.now(|r| Msg::GotToday(r.as_text().unwrap_or_default().to_string()));
}

Msg::Schema(json) => {                                 // json = [{"user_version":"N"}]
    model.pending_sql = migrations_from(pragma_int(&json)).into_iter().map(String::from).collect();
    run_migration(model, cx);
}
Msg::Migrated => { if !model.pending_sql.is_empty() { model.pending_sql.remove(0); } run_migration(model, cx); }

fn run_migration(model, cx) {
    match model.pending_sql.first() {
        Some(sql) => cx.plugin("sqlite", "exec", sql.clone(), |_| Msg::Migrated),
        None => load_all(cx),                          // queue drained → load accounts + txns
    }
}
```

## 2. One ledger, three kinds

Expenses became **transactions**. A `Txn` has a `kind` (`expense` / `income` / `transfer`), an
`account_id`, and — for transfers — a `to_account_id`:

```rust
pub enum TxnKind { Expense, Income, Transfer }

pub struct Txn {
    pub id: u32, pub ts: String, pub kind: TxnKind, pub amount: f64,
    pub account_id: u32, pub to_account_id: Option<u32>,
    pub category: String, pub note: String,
}
```

A column that's `NULL` in SQLite (`to_account_id` on non-transfers) comes back differently on each
platform — an empty string on iOS, an absent key on Android — so we parse defensively and let both mean
"none":

```rust
to_account_id: r.get("to_account_id").and_then(Value::as_str).and_then(|s| s.parse().ok()),
```

## 3. Balances and net worth are *derived*, not stored

We never store a running balance — that's a recipe for drift. An account's balance is computed from its
opening amount plus the transactions that touch it:

```rust
fn balance(acc: &Account, txns: &[Txn]) -> f64 {
    let mut b = acc.opening;
    for t in txns {
        match t.kind {
            TxnKind::Income   if t.account_id == acc.id => b += t.amount,
            TxnKind::Expense  if t.account_id == acc.id => b -= t.amount,
            TxnKind::Transfer => {
                if t.account_id == acc.id          { b -= t.amount; } // out
                if t.to_account_id == Some(acc.id) { b += t.amount; } // in
            }
            _ => {}
        }
    }
    b
}
```

Net worth is then `assets − liabilities`, summing each account's balance by kind. Transfers move money
between accounts but change neither net worth nor the income/expense totals — the Bills summary excludes
them deliberately.

## 4. The transaction sheet adapts to the kind

The add sheet now leads with a kind selector and shows account **chips**; for a transfer it swaps the
category field for a second "to account" picker:

```rust
fn txn_sheet(model) -> Widget {
    let mut items = vec![
        segmented(vec![ /* Expense | Income | Transfer */ ]),
        text_field("amount", "Amount", model.draft_amount.clone()),
        caption(if transfer { "From account" } else { "Account" }),
        account_chips(model, model.draft_account, Msg::SetAccount),
    ];
    if transfer {
        items.push(caption("To account"));
        items.push(account_chips(model, model.draft_to_account, Msg::SetToAccount));
    } else {
        items.push(text_field("category", "Category", model.draft_category.clone()));
    }
    // …note, Save…
}
```

Validation lives in the core and returns a friendly reason, surfaced as a toast:

```rust
fn validate_txn(model) -> Result<(), &'static str> {
    if parse_amount(&model.draft_amount).is_none() { return Err("Enter an amount greater than zero."); }
    if model.draft_account.is_none()               { return Err("Pick an account."); }
    if model.draft_kind == TxnKind::Transfer {
        match model.draft_to_account {
            None                               => return Err("Pick a destination account."),
            id if id == model.draft_account    => return Err("Pick two different accounts."),
            _ => {}
        }
    }
    Ok(())
}
```

## 5. The Assets tab

A summary card (Assets / Liabilities / Net worth) over a list of accounts and their live balances; its
FAB opens an "add account" sheet (name, asset-or-liability, opening balance). The Bills summary likewise
grew to **Income / Expense / Net** for the period.

## What we built

Saldo is now a genuine money manager: money lives in accounts, every entry is an income, expense, or
transfer, balances and net worth fall out of the data, and the Chapter-2 database upgrades itself in
place. All the money math (`balance`, `net_worth`, `period_totals`) is pure and unit-tested.

**Next:** Chapter 4 — Categories & the entry sheet: editable income/expense categories with
subcategories, a category picker, and a real date picker.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
