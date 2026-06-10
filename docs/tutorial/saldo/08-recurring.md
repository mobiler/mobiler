# Chapter 8 — Recurring transactions

Rent, salary, a subscription — lots of money moves on a schedule. This chapter lets Saldo **post those
automatically**: you mark an entry as repeating, and every time the app launches it posts whatever has
come due since you last opened it. No background jobs — just a rules table and a catch-up pass at startup.

## 1. A `recurring` table (schema v4)

A rule is a transaction template plus a cadence and a cursor (`next_date`, the next occurrence still to
post):

```rust
if from < SCHEMA_VERSION {
    // v4 — recurring rules. `next_date` is the next occurrence to post; materialization advances it.
    q.extend_from_slice(&[
        "CREATE TABLE IF NOT EXISTS recurring(id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, \
         amount TEXT NOT NULL, account_id INTEGER NOT NULL, to_account_id INTEGER, \
         category TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT '', freq TEXT NOT NULL, \
         next_date TEXT NOT NULL, end_date TEXT)",
        "PRAGMA user_version = 4",
    ]);
}
```

> **A migration gotcha worth internalizing.** Earlier the Chapter-3 block was gated `if from <
> SCHEMA_VERSION`. The moment we bump `SCHEMA_VERSION` to 4, that condition also matches a v3 device — so
> it would *re-run* the category seeds. The fix: **only the newest block gates on `SCHEMA_VERSION`; every
> earlier block gates on its own literal version** (`if from < 3`). A unit test pins this down.

## 2. Date arithmetic without a date library

Advancing `next_date` needs real calendar math (month lengths, leap years), but pulling in a date crate
for the wasm build is overkill. Howard Hinnant's `days_from_civil` / `civil_from_days` convert a date to a
day-count and back in a few lines, and everything else builds on them:

```rust
fn add_days(ymd: &str, n: i64) -> String {
    let Some((y, m, d)) = parse_ymd(ymd) else { return ymd.to_string() };
    let (ny, nm, nd) = civil_from_days(days_from_civil(y, m, d) + n);
    ymd_str(ny, nm, nd)
}

fn advance(ymd: &str, freq: Freq) -> String {
    match freq {
        Freq::Daily  => add_days(ymd, 1),
        Freq::Weekly => add_days(ymd, 7),
        Freq::Monthly => { /* +1 month, clamping the day to the month's length: Jan 31 → Feb 28/29 */ }
    }
}
```

Tests nail the edge cases — year rollover, `Jan 31 → Feb 28`, and `Jan 31 → Feb 29` in a leap year.

## 3. Materialization: a pure catch-up, then SQL

The core of the chapter is one pure function. Given the rules and today, it returns the transactions to
post and each rule's new cursor — looping so a rule dormant for months posts *all* the occurrences it
missed:

```rust
fn due_posts(rules: &[Recurring], today: &str) -> (Vec<DuePost>, Vec<(u32, String)>) {
    // for each rule: while next_date <= today (and <= end_date), emit a post and advance next_date
}
```

Being pure, it's trivially testable: a monthly rule with `next_date` in March, opened in June, yields four
posts (Mar–Jun) and advances to July; an `end_date` caps it; a future `next_date` yields nothing.

`materialize` turns that into SQL — an `INSERT` per post, an `UPDATE` per advanced cursor, then a reload:

```rust
fn materialize(model: &Model, cx: &mut Cx<Msg>) {
    if !model.recurring_loaded || model.today.is_empty() { return; } // need both halves
    let (posts, updates) = due_posts(&model.recurring, &model.today);
    if posts.is_empty() { return; }                                  // idempotent once caught up
    // …issue INSERT/UPDATE writes (callback Msg::Posted)…
    load_all(cx); // refresh; re-fires materialize, now a no-op
}
```

Two subtleties make this robust:

- **Both halves.** `today` (from `cx.now`) and the rules (from SQLite) arrive on independent async
  callbacks. So `materialize` is called from *both* `GotToday` and `RecurringLoaded`, and bails until
  both are present — whichever lands second runs the pass.
- **Idempotent.** Once a rule's `next_date` is advanced past today, `due_posts` returns nothing, so the
  reload that follows materialization re-fires it harmlessly. No "already posted?" bookkeeping needed.

The writes run before the reload because the `sqlite` plugin serializes on one connection — the queue we
already relied on for migrations.

## 4. Creating a rule from the entry sheet

Rather than a separate screen, the entry sheet grows a **Repeat** row — *Once* (a one-off, as before) or
a cadence:

```rust
items.push(caption(tr(model, "field.repeat")));
items.push(segmented(vec![
    segment(tr(model, "freq.once"),    model.draft_freq.is_none(),               Msg::SetFreq(None)),
    segment(tr(model, "freq.daily"),   model.draft_freq == Some(Freq::Daily),    Msg::SetFreq(Some(Freq::Daily))),
    // …weekly, monthly…
]));
```

On save, the same drafted fields become either a `txn` or a `recurring` row:

```rust
let sql = if let Some(freq) = model.draft_freq {
    json!({ "sql": "INSERT INTO recurring(…, freq, next_date, end_date) VALUES (…, ?, ?, NULL)", … })
} else {
    json!({ "sql": "INSERT INTO txn(…) VALUES (…)", … })
};
```

Because a new rule's `next_date` is the chosen date (today by default), the reload right after saving
materializes its **first** occurrence immediately and schedules the next — creating a monthly rule today
posts today's transaction and sets `next_date` to next month.

## 5. Managing rules

The Settings tab — which got a language picker in Chapter 7 — gains a **Scheduled** card listing each
rule (amount, category/transfer, cadence, next date) with a Delete. Deleting a rule stops future posts;
the transactions it already created stay in the ledger:

```rust
Msg::DeleteRecurring(id) => { /* DELETE FROM recurring WHERE id = ? → Reload */ }
```

## What we built

Scheduled transactions, end to end: a rules table, a dependency-free date engine, a pure catch-up
materializer that posts everything due since last launch (idempotently, once both `today` and the rules
are loaded), rule creation folded into the existing entry sheet, and a management list in Settings — all
covered by unit tests (the date math, `due_posts` catch-up / end-date / future-rule cases, and the
migration-gate fix).

**Next:** Chapter 9 — Data: CSV export, CSV import, and full JSON backup/restore (the `files` and
`filepicker` plugins).

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
