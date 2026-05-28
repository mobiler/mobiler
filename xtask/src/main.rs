//! Dev tooling. `cargo run -p xtask -- gen-readme [--check]` rebuilds the
//! capability sections of the READMEs from `capabilities.json` (the single source of
//! truth). `--check` (used in CI) fails if any README is out of sync instead of writing.
//!
//! Each README marks its generated region with HTML comments; the `format=` on the
//! start marker picks the rendering:
//!   <!-- capabilities:start format=table -->  ... <!-- capabilities:end -->   (md table)
//!   <!-- capabilities:start format=inline -->  ... <!-- capabilities:end -->  (prose list)

use std::{fs, path::PathBuf, process::exit};

use serde::Deserialize;

#[derive(Deserialize)]
struct Registry {
    capabilities: Vec<Capability>,
}

#[derive(Deserialize)]
#[allow(dead_code)] // `plugin`/`since` are tracked in the registry but not rendered.
struct Capability {
    name: String,
    short: String,
    api: String,
    plugin: String,
    tier: String,
    status: String,
    #[serde(default)]
    since: String,
    notes: String,
}

/// READMEs that carry a generated capability region.
const TARGETS: &[&str] = &[
    "README.md",
    "mobiler-core/README.md",
    "mobiler/README.md",
    "mobiler-web/README.md",
];

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("gen-readme") {
        eprintln!("usage: cargo run -p xtask -- gen-readme [--check]");
        exit(2);
    }
    let check = args.any(|a| a == "--check");

    // Repo root = parent of this crate (xtask/), regardless of the caller's CWD.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf();

    let registry: Registry = serde_json::from_str(
        &fs::read_to_string(root.join("capabilities.json")).expect("read capabilities.json"),
    )
    .expect("parse capabilities.json");

    // The public, available set — what an app can call today.
    let caps: Vec<&Capability> = registry
        .capabilities
        .iter()
        .filter(|c| c.tier == "free" && c.status == "shipped")
        .collect();

    let mut stale = Vec::new();
    for target in TARGETS {
        let path = root.join(target);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {target}: {e}"));
        let updated = rewrite(&content, &caps).unwrap_or_else(|e| {
            eprintln!("{target}: {e}");
            exit(2);
        });
        if updated == content {
            if !check {
                println!("unchanged  {target}");
            }
        } else if check {
            stale.push(*target);
        } else {
            fs::write(&path, updated).unwrap_or_else(|e| panic!("write {target}: {e}"));
            println!("updated    {target}");
        }
    }

    if check {
        if stale.is_empty() {
            println!("READMEs are in sync with capabilities.json");
        } else {
            eprintln!("ERROR: READMEs out of sync with capabilities.json: {}", stale.join(", "));
            eprintln!("fix with: cargo run -p xtask -- gen-readme");
            exit(1);
        }
    }
}

/// Replace the text between the capability markers with freshly rendered content.
fn rewrite(content: &str, caps: &[&Capability]) -> Result<String, String> {
    const START: &str = "<!-- capabilities:start";
    const END: &str = "<!-- capabilities:end -->";

    let start = content.find(START).ok_or("missing `capabilities:start` marker")?;
    let after_start = content[start..].find("-->").ok_or("unterminated start marker")? + start + 3;
    let start_marker = &content[start..after_start];
    let end = content[after_start..].find(END).ok_or("missing `capabilities:end` marker")? + after_start;

    // Leading whitespace of the start-marker line, re-applied to the generated block and
    // the end marker — so a region living inside a markdown bullet stays indented.
    let line_start = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent = &content[line_start..start];

    let raw = if start_marker.contains("format=table") {
        render_table(caps)
    } else {
        render_inline(caps)
    };
    let block = raw
        .lines()
        .map(|l| if l.is_empty() { String::new() } else { format!("{indent}{l}") })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("{}\n{}\n{}{}", &content[..after_start], block, indent, &content[end..]))
}

fn render_table(caps: &[&Capability]) -> String {
    let mut s = String::from("| Capability | Rust API | Notes |\n|---|---|---|");
    for c in caps {
        s.push_str(&format!("\n| {} | `{}` | {} |", c.name, c.api, c.notes));
    }
    s
}

/// A prose fragment: "HTTP, storage, …, and camera capture."
fn render_inline(caps: &[&Capability]) -> String {
    let shorts: Vec<&str> = caps.iter().map(|c| c.short.as_str()).collect();
    let list = match shorts.as_slice() {
        [] => String::new(),
        [only] => only.to_string(),
        [head @ .., last] => format!("{}, and {}", head.join(", "), last),
    };
    format!("{list}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(name: &str, short: &str, api: &str, notes: &str) -> Capability {
        Capability {
            name: name.into(),
            short: short.into(),
            api: api.into(),
            plugin: "p".into(),
            tier: "free".into(),
            status: "shipped".into(),
            since: "0.1.0".into(),
            notes: notes.into(),
        }
    }

    #[test]
    fn render_inline_uses_an_oxford_and() {
        let (a, b, c) = (cap("A", "aye", "x", "n"), cap("B", "bee", "y", "m"), cap("C", "cee", "z", "o"));
        assert_eq!(render_inline(&[]), ".");
        assert_eq!(render_inline(&[&a]), "aye.");
        assert_eq!(render_inline(&[&a, &b]), "aye, and bee.");
        assert_eq!(render_inline(&[&a, &b, &c]), "aye, bee, and cee.");
    }

    #[test]
    fn render_table_has_a_header_and_a_row_per_capability() {
        let t = render_table(&[&cap("HTTP", "http", "cx.get", "req")]);
        assert!(t.starts_with("| Capability | Rust API | Notes |"));
        assert!(t.contains("| HTTP | `cx.get` | req |"));
    }

    #[test]
    fn rewrite_replaces_between_markers_keeps_surroundings_and_errors_without_them() {
        let caps = [&cap("HTTP", "http", "cx.get", "req")];
        let content = "intro\n<!-- capabilities:start format=inline -->\nOLD\n<!-- capabilities:end -->\noutro\n";
        let out = rewrite(content, &caps).unwrap();
        assert!(out.contains("http."), "generated content present");
        assert!(!out.contains("OLD"), "stale content replaced");
        assert!(out.contains("intro") && out.contains("outro"), "surrounding text preserved");
        assert!(out.contains("<!-- capabilities:start") && out.contains("<!-- capabilities:end -->"), "markers kept");
        // No markers → a clear error, never a panic.
        assert!(rewrite("no markers here", &caps).is_err());
    }
}
