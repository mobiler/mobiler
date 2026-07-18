# Chapter 2 — Persist with SQLite

In [Chapter 1](01-scaffold-and-first-screen.md) the ledger lived in memory — quit the app and it was
gone. Now we make it durable with on-device **SQLite**, stamp each entry with the current date-time,
group the list by day, and filter by period. Two new ideas carry the chapter:

1. A Mobiler **app core is pure** — it can't read the disk *or* the clock. Both go through
   capabilities: the `sqlite` plugin and a new `cx.now()`.
2. Because the core is pure, **the `Model` isn't persisted** — SQLite is. We re-read the ledger from
   the database on every launch.

## 1. Add the SQLite plugin

`sqlite` is a bundled plugin (not a built-in), so we wire it into the app's native shells once:

```sh
mobiler plugin add sqlite
```

That injects a `SqlitePlugin` into the iOS and Android shells and registers it — no ABI or
generated-binding changes, just the shell's plugin registry. Now `cx.plugin("sqlite", …)` resolves.

The contract is tiny:

- **`exec`** runs a statement (`CREATE` / `INSERT` / `UPDATE` / `DELETE`).
- **`query`** runs a `SELECT` and returns a **JSON array of rows**, where every value is a **string**
  (SQLite text affinity over the bridge).
- Input is either raw SQL or `{"sql": "...", "args": [...]}` with **bound parameters** — always use
  `args` for anything user-typed.
- There's one database file (`mobiler.db`); the **app owns its schema**.

## 2. A capability for "now"

A finance app stamps each entry with the date it happened — but the pure core has no clock. Saldo needs
"today" without making the user open a date picker, so the framework grows a small capability:

```rust
// in mobiler_core: a built-in `datetime` op with no UI
cx.now(|r| Msg::Stamped(r.as_text().unwrap_or_default().to_string())); // "YYYY-MM-DD HH:MM:SS" (local)
```

`cx.now()` resolves immediately (no picker) with a sortable local timestamp — and its first 10
characters are the day, which is all we need for grouping. (This is a nice illustration of the
"build the app, grow the framework when it's genuinely missing something" loop — `now` is useful to
*any* app that logs events, so it lives in the framework, not in Saldo.)

## 3. The schema, created on launch

```rust
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS expense(\
    id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, amount TEXT NOT NULL, \
    category TEXT NOT NULL, note TEXT NOT NULL DEFAULT '')";
```

`MobilerApp::init` runs once at startup — the place to create the table and load:

```rust
fn init(&self, _model: &mut Model, cx: &mut Cx<Msg>) {
    cx.plugin("sqlite", "exec", SCHEMA, |_| Msg::Reload); // create, then load
    cx.now(|r| Msg::GotToday(r.as_text().unwrap_or_default().to_string())); // stamp "today" for the filter
}
```

`IF NOT EXISTS` makes that idempotent — it's our cheap migration story (later chapters add tables the
same way).

## 4. The write path: validate → stamp → insert → reload

Adding an expense is a little pipeline of capability round-trips, each step a `Msg`:

```rust
Msg::Save => {
    if parse_amount(&model.draft_amount).is_none() {          // validate in the core
        cx.notify("toast", "show", "Enter an amount greater than zero.");
        return;
    }
    cx.now(|r| Msg::Stamped(if r.ok { r.as_text().unwrap_or_default().to_string() } else { String::new() }));
}
Msg::Stamped(ts) => {
    // bound parameters — never concatenate user text into SQL
    let sql = serde_json::json!({
        "sql": "INSERT INTO expense(ts, amount, category, note) VALUES (?, ?, ?, ?)",
        "args": [ts, amount.to_string(), category, model.draft_note.trim()],
    }).to_string();
    cx.plugin("sqlite", "exec", sql, |r| Msg::Saved(r.ok));
}
Msg::Saved(ok) => if ok { model.adding = false; load(cx); } // re-read after the write
```

Deleting is the same shape — an `exec` with a bound id, then a reload:

```rust
Msg::Delete(id) => {
    let sql = serde_json::json!({ "sql": "DELETE FROM expense WHERE id = ?",
                                  "args": [id.to_string()] }).to_string();
    cx.plugin("sqlite", "exec", sql, |_| Msg::Reload);
}
```

## 5. The read path: query → parse → model

`load` is shared by init, save, and delete:

```rust
fn load(cx: &mut Cx<Msg>) {
    cx.plugin("sqlite", "query",
        "SELECT id, ts, amount, category, note FROM expense ORDER BY ts DESC",
        |r| Msg::Loaded(if r.ok { r.as_text().unwrap_or_default().to_string() } else { "[]".to_string() }));
}
```

Rows come back as JSON **strings**, so parsing means pulling each column and converting:

```rust
fn parse_rows(json: &str) -> Vec<Expense> {
    serde_json::from_str::<Vec<serde_json::Value>>(json).unwrap_or_default().iter()
        .filter_map(|r| Some(Expense {
            id:       r.get("id")?.as_str()?.parse().ok()?,
            ts:       r.get("ts")?.as_str()?.to_string(),
            amount:   r.get("amount")?.as_str()?.parse().ok()?,
            category: r.get("category")?.as_str()?.to_string(),
            note:     r.get("note").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
        }))
        .collect()
}
```

`filter_map` + `?` means a malformed row is skipped, never a panic.

## 6. Group by day, filter by period

The view derives everything from the loaded `Vec<Expense>`. Since the query is `ORDER BY ts DESC`,
same-day entries are already adjacent, so grouping is a single pass:

```rust
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
```

A `segmented` control switches the period (Day / Month / All), compared against the `today` string we
stamped at startup:

```rust
fn in_period(e: &Expense, period: Period, today: &str) -> bool {
    match period {
        Period::All   => true,
        Period::Day   => e.day() == today,                       // first 10 chars
        Period::Month => !today.is_empty() && e.ts.get(..7) == today.get(..7),
    }
}
```

String-prefix comparison is enough because the timestamp is ISO-ordered. (Week-based periods, which
need real date math, come later.)

## 7. Test the logic without a device

The pure helpers — parsing, filtering, grouping — are plain functions, so they unit-test with no
simulator:

```rust
#[test]
fn period_filter() {
    let xs = parse_rows(ROWS);
    let today = "2026-06-09";
    let count = |p| xs.iter().filter(|e| in_period(e, p, today)).count();
    assert_eq!(count(Period::Day), 2);
    assert_eq!(count(Period::Month), 3);
}
```

## What we built

Expenses now survive a restart, carry a real date, group under day headers with subtotals, and filter
by Day / Month / All — and we grew the framework a `cx.now()` capability along the way. The data lives
in SQLite; the `Model` is just a cache we rebuild on launch.

**Next:** Chapter 3 — Accounts & transfers: the Assets tab, balances, net worth, and moving money
between accounts.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
