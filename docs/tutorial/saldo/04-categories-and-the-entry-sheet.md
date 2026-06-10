# Chapter 4 — Categories & the entry sheet

Chapter 3 left categories as a free-text field. Real money managers ship a **category tree** — top-level
categories with subcategories, separate sets for income and expense — and let you **back-date** an entry.
This chapter adds a seeded `category` table, a two-level category **picker**, and a native **date
picker**.

## 1. A categories table, seeded by a migration

Bumping `SCHEMA_VERSION` to 3 adds the table and a default set. Because migrations run **in order** and
**once**, we can seed unconditionally (no `WHERE NOT EXISTS` needed) and reference a parent we inserted
moments earlier by name:

```rust
if from < SCHEMA_VERSION {
    q.extend_from_slice(&[
        "CREATE TABLE IF NOT EXISTS category(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, \
         kind TEXT NOT NULL, parent_id INTEGER, sort INTEGER NOT NULL DEFAULT 0)",
        "INSERT INTO category(name, kind, sort) VALUES ('Food & Drink','expense',1)",
        // …more top-level categories…
        "INSERT INTO category(name, kind, parent_id, sort) \
         SELECT 'Groceries','expense',id,1 FROM category WHERE name='Food & Drink' AND kind='expense'",
        // …more subcategories…
        "PRAGMA user_version = 3",
    ]);
}
```

A device on v2 (from Chapter 3) runs only this block; a fresh install runs v2 then v3 back to back.
`Category` carries an optional `parent_id`:

```rust
pub struct Category { pub id: u32, pub name: String, pub kind: String, pub parent_id: Option<u32> }
```

## 2. A two-level category picker

The picker shows the top-level categories for the current kind as chips, and — for whichever top-level
the current selection belongs to — its subcategories underneath. The trick is one helper that maps the
selected name back to the parent whose children should be visible:

```rust
fn open_parent(categories: &[Category], kind: &str, selected: &str) -> Option<u32> {
    categories.iter()
        .find(|c| c.kind == kind && c.name == selected)
        .map(|c| c.parent_id.unwrap_or(c.id)) // a subcategory → its parent; a top-level → itself
}
```

So picking *Food & Drink* reveals *Groceries / Restaurants / Coffee*; picking *Coffee* keeps that row
open and marks Coffee selected. Either level stores its **name** in `draft_category`:

```rust
fn category_picker(model: &Model) -> Widget {
    let kind = model.draft_kind.category_kind(); // income → "income", else "expense"
    let sel = &model.draft_category;
    let tops = model.categories.iter()
        .filter(|c| c.kind == kind && c.parent_id.is_none())
        .map(|c| chip(c.name.clone(), &c.name == sel, Msg::SetCategory(c.name.clone())));
    let mut items = vec![caption("Category"), row(tops.collect())];
    if let Some(pid) = open_parent(&model.categories, kind, sel) {
        let kids = model.categories.iter()
            .filter(|c| c.kind == kind && c.parent_id == Some(pid))
            .map(|c| chip(c.name.clone(), &c.name == sel, Msg::SetCategory(c.name.clone())));
        // …push the subcategory row if any…
    }
    column(items)
}
```

Switching the transaction kind clears the selection, since income and expense have different category
sets:

```rust
Msg::SetKind(k) => { model.draft_kind = k; model.draft_category.clear(); }
```

## 3. A native date picker

Entries default to today (the `today` we stamped with `cx.now()` at startup), but you can back-date them
with the system date picker — that's the built-in `cx.pick_date` capability from Chapter 2's `datetime`
plugin:

```rust
Msg::PickDate         => cx.pick_date(|r| Msg::DatePicked(if r.ok { r.output } else { String::new() })),
Msg::DatePicked(date) => if !date.is_empty() { model.draft_date = date; },
```

On save we build the timestamp from the chosen date (entries are date-precision; ordering within a day
falls back to the row id via `ORDER BY ts DESC, id DESC`):

```rust
let ts = format!("{} 12:00:00", model.draft_date);
```

The sheet shows the current date with a *Change* button:

```rust
row(vec![caption("Date"), spacer(Spacing::Md), text(model.draft_date.clone()),
         button("Change", ButtonStyle::Text, Msg::PickDate)])
```

## 4. Validation grows a category rule

Income and expense now require a category; transfers still don't:

```rust
if model.draft_kind == TxnKind::Transfer {
    // …needs two distinct accounts…
} else if model.draft_category.trim().is_empty() {
    return Err("Pick a category.");
}
```

## What we built

A real category tree (seeded, income vs expense, two levels), a picker that reveals subcategories, and a
date picker for back-dating — all unit-tested at the logic level (`open_parent`, category parsing, the
new validation rule). Managing categories (rename/add/delete) lands with the Settings screen later; the
data model is already in place.

**Next:** Chapter 5 — Stats: a category breakdown pie/donut, a net-worth trend line, and monthly bars,
all from the `Chart` widget.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
