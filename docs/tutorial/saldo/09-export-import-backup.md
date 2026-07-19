# Chapter 9 — Export & backup

Your money data should be *yours* — easy to pull into a spreadsheet, and safe to move to a new phone.
This chapter adds two things to a new **Data** card in Settings: a **CSV export** of the ledger, and a
full **JSON backup / restore**. Both lean on two bundled plugins we wire in here — `files` (write to the
sandbox + hand a file to the system *save* picker) and `filepicker` (pick a file back).

## 1. Wiring the plugins

`files` and `filepicker` aren't in the default scaffold, so add them once:

```sh
mobiler plugin add files
mobiler plugin add filepicker
```

That drops the native implementations into the iOS/Android shells and registers them, so
`cx.plugin("files", …)` and `cx.plugin("filepicker", …)` resolve. Their shapes:

- `files` `write` `{"path","content"}` → writes to the app sandbox; `export` `{"path","name"}` → opens the
  system save sheet; `read` `{"path"}` → returns the file's text (and accepts a `file://` URI).
- `filepicker` `pick` `""` → returns a `file://` URI for the chosen file (content is then read via `files`).

## 2. CSV export — build in the core, then save

Export is two hops: write the text to the sandbox, then (on success) open the save picker. A small helper
threads the *save-as* name through the write's callback:

```rust
fn write_and_export(cx: &mut Cx<Msg>, path: &str, name: String, content: &str) {
    let p = path.to_string();
    let input = json!({ "path": path, "content": content }).to_string();
    cx.plugin("files", "write", input, move |r| {
        if r.ok { Msg::DoExport(p, name) } else { Msg::ExportDone(false) }
    });
}

Msg::ExportCsv  => write_and_export(cx, "saldo-export.csv", format!("Saldo-{}.csv", model.today), &build_csv(model)),
Msg::DoExport(path, name) => cx.plugin("files", "export", json!({ "path": path, "name": name }).to_string(),
                                       |r| Msg::ExportDone(r.ok)),
Msg::ExportDone(ok) => cx.notify("toast", "show", tr(model, if ok { "data.saved" } else { "data.cancelled" })),
```

The CSV itself is a pure function — a header plus a row per transaction, with proper quoting for fields
that contain a comma, quote, or newline:

```rust
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}
```

Because it's pure, a test pins the header, the row count, and that `"Bank, EU"` comes out quoted.

## 3. JSON backup — serialize every table

CSV is lossy (it flattens to text); a *backup* must restore exactly, so it's the raw tables as JSON. We
derive `Serialize`/`Deserialize` on the row structs and dump them through one snapshot type:

```rust
#[derive(Serialize, Deserialize)]
struct Backup { version: u32, accounts: Vec<Account>, categories: Vec<Category>, txns: Vec<Txn>, recurring: Vec<Recurring> }

fn build_backup(model: &Model) -> String {
    serde_json::to_string(&Backup { version: SCHEMA_VERSION, accounts: model.accounts.clone(), /* … */ }).unwrap_or_default()
}
```

It rides the exact same `write_and_export` path as CSV — only the content and filename differ.

## 4. Restore — pick, read, replace

Restoring is the mirror image: pick the file, read its text, parse it, and rebuild every table. The
rebuild is a pure function returning ready-to-run SQL — clear all four tables, then re-insert each row
**with its original id** so `account_id` / `parent_id` references still line up:

```rust
fn restore_statements(b: &Backup) -> Vec<String> {
    // DELETE FROM txn / recurring / category / account, then INSERT … (id, …) for every row
}
```

The three async hops are three messages:

```rust
Msg::Restore        => cx.plugin("filepicker", "pick", "", |r| if r.ok { Msg::RestorePicked(r.as_text().unwrap_or_default().to_string()) } else { Msg::ExportDone(false) }),
Msg::RestorePicked(uri) => cx.plugin("files", "read", json!({ "path": uri }).to_string(),
                                     |r| Msg::RestoreRead(if r.ok { r.as_text().unwrap_or_default().to_string() } else { String::new() })),
Msg::RestoreRead(json) => match serde_json::from_str::<Backup>(&json) {
    Ok(backup) => { for stmt in restore_statements(&backup) { cx.plugin("sqlite", "exec", stmt, |_| Msg::Posted); } load_all(cx); }
    Err(_)     => cx.notify("toast", "show", tr(model, "data.bad_backup")),
},
```

The writes run before `load_all` because the `sqlite` plugin serializes on one connection — the same
ordering guarantee we relied on for migrations and recurring materialization. A bad/foreign file fails the
`from_str` and just toasts an error; it never half-applies.

## What we built (and what we didn't)

A **Data** card in Settings: **Export CSV** (ledger → spreadsheet), **Back up** (everything → JSON), and
**Restore from backup** (JSON → everything, ids preserved). The pure pieces — CSV building/quoting, the
backup round-trip through `restore_statements` — are unit-tested.

We deliberately **left out importing arbitrary third-party CSVs**: mapping foreign columns, currencies,
and category names onto our schema is a validation problem worth its own chapter. JSON backup/restore is
the lossless round-trip; CSV stays an *export*.

**Next:** Chapter 10 — Security, settings & polish: a passcode / biometric lock, the rounded-out Settings
screen, and the Saldo theme, icons, and dark mode.

> Full source for this chapter: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs).
