# Chapter 5 — Stats

The ledger and accounts are in place, so we have data worth looking at. This chapter turns the empty
**Stats** tab into three charts — a **category breakdown** donut, a **net-worth trend** line, and a
**monthly income-vs-expense** bar chart — all from the built-in `Chart` widget. The charts are pure
functions of the `txns`/`accounts`/`categories` already in the `Model`, so every aggregation is a small,
unit-tested helper.

## 1. A little state, one new message

Stats reuses the **same period** as the Bills tab (`model.period`), so switching Day/Month/All in either
place keeps them in sync. The only new state is which side to break down — expenses or income:

```rust
pub struct Model {
    // …
    stats_kind: TxnKind, // expense/income toggle on the Stats tab
    // …
}
```

```rust
Msg::SetStatsKind(k) => model.stats_kind = k,
```

`TxnKind` already defaults to `Expense`, which is exactly what we want the breakdown to open on.

## 2. Breaking spending down by category

Transactions store the **leaf** category name (e.g. `"Groceries"`). A breakdown chart reads better rolled
up to the **top-level** category (`"Food & Drink"`), so one helper maps a leaf name to its parent's name —
or returns the name unchanged for a top-level or unknown category:

```rust
fn top_category<'a>(categories: &'a [Category], kind: &str, name: &'a str) -> &'a str {
    let Some(c) = categories.iter().find(|c| c.kind == kind && c.name == name) else {
        return name;
    };
    match c.parent_id {
        Some(pid) => categories.iter().find(|p| p.id == pid).map_or(name, |p| p.name.as_str()),
        None => name,
    }
}
```

`category_breakdown` then sums the in-period transactions of the chosen kind by top-level name and sorts
the result by amount, biggest first (reusing `in_period` from Chapter 2):

```rust
fn category_breakdown(model: &Model, kind: TxnKind) -> Vec<(String, f64)> {
    let ck = kind.category_kind();
    let mut totals: Vec<(String, f64)> = Vec::new();
    for t in model.txns.iter().filter(|t| t.kind == kind && in_period(&t.ts, model.period, &model.today)) {
        let name = top_category(&model.categories, ck, &t.category).to_string();
        match totals.iter_mut().find(|(n, _)| *n == name) {
            Some((_, amt)) => *amt += t.amount,
            None => totals.push((name, t.amount)),
        }
    }
    totals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    totals
}
```

A donut takes one `ChartSeries` per slice. We give each slice an explicit colour from a small palette so
the chart and the ranked list below it line up, and add a percentage to each row:

```rust
let donut = donut_chart(
    breakdown.iter().enumerate()
        .map(|(i, (name, amt))| ChartSeries::new(name.clone(), vec![*amt as f32]).with_color(palette(i)))
        .collect(),
);
// …then, per entry, a row of name · amount · "{pct:.0}%"
```

When nothing matches the period we show a caption (`"No expenses in this period."`) instead of an empty
chart — the same empty-state habit as the Bills and Assets tabs.

## 3. A net-worth trend over the last six months

The trend needs net worth *as of* each past month. `month_tags` lists the last N `"YYYY-MM"` tags,
oldest→newest, walking the month back and rolling the year over at January:

```rust
fn month_tags(today: &str, n: usize) -> Vec<String> {
    let (Some(mut y), Some(mut m)) = ( /* parse YYYY and MM from `today` */ ) else { return Vec::new() };
    let mut tags = Vec::with_capacity(n);
    for _ in 0..n {
        tags.push(format!("{y:04}-{m:02}"));
        m -= 1;
        if m == 0 { m = 12; y -= 1; }
    }
    tags.reverse();
    tags
}
```

Because timestamps are stored as `"YYYY-MM-DD HH:MM:SS"`, "everything up to the end of a month" is a plain
**lexical** string compare — no date library needed. We filter to that cutoff and reuse Chapter 3's
`net_worth`:

```rust
fn net_worth_asof(accounts: &[Account], txns: &[Txn], tag: &str) -> f64 {
    let cutoff = format!("{tag}-31 23:59:59");
    let upto: Vec<Txn> = txns.iter().filter(|t| t.ts.as_str() <= cutoff.as_str()).cloned().collect();
    net_worth(accounts, &upto).2
}
```

That feeds a single-series line chart (axis on, legend off), labelled by month number:

```rust
chart(vec![ChartSeries::new("Net worth", trend)], labels, ChartStyle::Line, true, false)
```

## 4. Monthly income vs expense

The last chart compares income and expense per month. `monthly_totals` sums one month's tag:

```rust
fn monthly_totals(txns: &[Txn], tag: &str) -> (f64, f64) {
    let (mut income, mut expense) = (0.0, 0.0);
    for t in txns.iter().filter(|t| t.ts.get(..7) == Some(tag)) {
        match t.kind {
            TxnKind::Income => income += t.amount,
            TxnKind::Expense => expense += t.amount,
            TxnKind::Transfer => {}
        }
    }
    (income, expense)
}
```

Two series over the same six month labels give grouped bars (`ChartStyle::Bar`, axis + legend on):

```rust
chart(
    vec![
        ChartSeries::new("Income", inc).with_color(palette(2)),  // green
        ChartSeries::new("Expense", exp).with_color(palette(1)), // red
    ],
    labels, ChartStyle::Bar, true, true,
)
```

The same `chart`/`ChartSeries`/`ChartStyle` API renders to SwiftUI's `Canvas`, Compose's `Canvas`, and an
SVG on the web — Saldo just describes the data.

## 5. All logic, all testable

Because the charts are built from pure helpers, the interesting behaviour is covered without a UI: that
`month_tags` rolls the year over (`2026-01` back three → `2025-11, 2025-12, 2026-01`), that
`net_worth_asof` excludes transactions after its cutoff, that `category_breakdown` folds subcategories
into their parent and sorts descending, and that `monthly_totals` splits a month's income from its
expense.

## What we built

A real Stats screen — a category donut with a ranked, percentage list (income/expense toggle, shared
period), a six-month net-worth trend, and monthly income-vs-expense bars — entirely from the existing
`Chart` widget and a handful of pure aggregation functions, no framework changes.

**Next:** Chapter 6 — Money & formatting: locale-aware currency and dates with `mobiler_core::format`,
and adding **Ukrainian** to the framework.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
