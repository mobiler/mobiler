//! Keeps `docs/adr/` well-formed: the rules in `docs/adr/README.md` that a machine can check.
//!
//! - Files are `ADR-NNNN-slug.md`, numbered from 0001 with no gaps or duplicates, and each title
//!   carries its own number.
//! - Every record has the header fields (`Status`, `Date decided` as a real `YYYY-MM-DD`, `Deciding
//!   PRs`, `Supersedes`, `Code anchor`, `Conformance`) and the five numbered sections.
//! - `Status` uses the vocabulary; `Superseded by ADR-NNNN` and `Supersedes: ADR-NNNN` name other,
//!   existing records.
//! - `index.md` has exactly one row per record, with the same status, date and superseded-by.
//! - Every `Conformance:` entry resolves: `path::test` → the file contains `fn test(`; a CI job
//!   reference → the job name appears in the workflow file; or `none — <why>`.
//!
//! Mutation proof (this guard belongs to the ADR set as a whole, so its proof lives here):
//! - Renaming `ADR-0010-…` to `ADR-0011-…` failed `numbering_is_contiguous_from_one` ("gap or
//!   duplicate: expected ADR-0010, found ADR-0011-app-constraints-are-opt-in-plugins.md"), and the
//!   index and well-formed checks with it.
//! - Deleting the ADR-0007 row from `index.md` failed `index_matches_the_records` ("ADR-0007
//!   missing from index.md").
//! - Changing ADR-0005's `Status:` to `Approved` failed `every_record_is_well_formed`
//!   ("ADR-0005: unknown status `Approved`").
//! - Renaming the test `shell_renders_before_requests_notifications_and_streams` in
//!   `mobiler-core/src/lib.rs` failed `conformance_entries_resolve` ("ADR-0005: Conformance cites
//!   `shell_renders_before_requests_notifications_and_streams`, which isn't a #[test] fn in
//!   mobiler-core/src/lib.rs").
//! - Setting ADR-0004's `Date decided:` to `2026-13-45` failed `every_record_is_well_formed`
//!   ("ADR-0004: `Date decided:` must be a real YYYY-MM-DD").
//!
//! - Renaming that test and keeping its old name in a `//` comment failed the same way ("… which
//!   isn't a #[test] fn …"); so did removing its `#[test]` attribute.
//! - Renaming the CI job `scaffold + build (template, Android)` and keeping the old name in a YAML
//!   comment failed `conformance_entries_resolve` ("… which isn't a `name:` in
//!   .github/workflows/ci.yml").
//! - `Date decided: 2026-02-31` failed ("must be a real YYYY-MM-DD").
//! - Marking ADR-0005 `Superseded by ADR-0009` without ADR-0009 naming it in `Supersedes:` failed
//!   ("superseded by ADR-0009, whose `Supersedes:` doesn't name it").
//! - An extra `|ADR-0099 | …` index row (no space after the pipe) failed `index_matches_the_records`
//!   ("index.md lists records that don't exist: [99]").
//!
//! Each was reverted and the suite went green again.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const HEADERS: [&str; 6] = ["Status:", "Date decided:", "Deciding PRs:", "Supersedes:", "Code anchor:", "Conformance:"];
const SECTIONS: [&str; 5] = ["## 1.", "## 2.", "## 3.", "## 4.", "## 5."];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("repo root").to_path_buf()
}

struct Record {
    number: u32,
    file: String,
    text: String,
}

fn records() -> Vec<Record> {
    let dir = root().join("docs/adr");
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("read docs/adr") {
        let name = entry.expect("dir entry").file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix("ADR-") else { continue };
        let digits: String = rest.chars().take(4).collect();
        assert!(
            digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) && rest[4..].starts_with('-') && name.ends_with(".md"),
            "{name}: records are named ADR-NNNN-slug.md"
        );
        let text = fs::read_to_string(dir.join(&name)).expect("read record");
        out.push(Record { number: digits.parse().expect("number"), file: name, text });
    }
    out.sort_by_key(|r| r.number);
    out
}

/// A header field, read only from the header block (before `## 1.`), so a line in the body can't
/// stand in for it.
fn header<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().take_while(|l| !l.starts_with("## 1.")).find_map(|l| l.strip_prefix(key)).map(str::trim)
}

/// A real calendar date, YYYY-MM-DD (days per month, leap years included).
fn is_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let num = |p: &str| p.parse::<u32>().ok();
    let (Some(y), Some(m), Some(d)) = (num(parts[0]), num(parts[1]), num(parts[2])) else { return false };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// `ADR-NNNN` → NNNN, for a reference to another record.
fn adr_ref(s: &str) -> Option<u32> {
    s.strip_prefix("ADR-").filter(|n| n.len() == 4).and_then(|n| n.parse().ok())
}

#[test]
fn numbering_is_contiguous_from_one() {
    let recs = records();
    assert!(!recs.is_empty(), "no records in docs/adr");
    for (i, r) in recs.iter().enumerate() {
        let expected = u32::try_from(i).expect("count") + 1;
        assert_eq!(r.number, expected, "gap or duplicate: expected ADR-{expected:04}, found {}", r.file);
    }
}

#[test]
fn every_record_is_well_formed() {
    let recs = records();
    let numbers: Vec<u32> = recs.iter().map(|r| r.number).collect();
    for r in &recs {
        let id = format!("ADR-{:04}", r.number);
        assert!(r.text.starts_with(&format!("# {id}: ")), "{}: the title must start with `# {id}: `", r.file);
        for h in HEADERS {
            let v = header(&r.text, h).unwrap_or_else(|| panic!("{id}: missing header `{h}`"));
            assert!(!v.is_empty(), "{id}: empty header `{h}`");
        }
        assert!(is_date(header(&r.text, "Date decided:").unwrap()), "{id}: `Date decided:` must be a real YYYY-MM-DD");
        let status = header(&r.text, "Status:").unwrap();
        let ok = match status {
            "Accepted" | "Accepted (blocked)" | "Deprecated" => true,
            s => s
                .strip_prefix("Superseded by ")
                .and_then(adr_ref)
                .is_some_and(|n| numbers.contains(&n) && n != r.number),
        };
        assert!(ok, "{id}: unknown status `{status}`");
        // A record marked superseded must be named in its successor's `Supersedes:`.
        if let Some(succ) = status.strip_prefix("Superseded by ").and_then(adr_ref) {
            let succ_rec = recs.iter().find(|x| x.number == succ).expect("successor exists");
            let back = header(&succ_rec.text, "Supersedes:").unwrap_or_default();
            assert!(back.split(", ").any(|x| adr_ref(x) == Some(r.number)), "{id}: superseded by ADR-{succ:04}, whose `Supersedes:` doesn't name it");
        }
        let supersedes = header(&r.text, "Supersedes:").unwrap();
        if supersedes != "none" {
            for item in supersedes.split(", ") {
                let n = adr_ref(item).unwrap_or_else(|| panic!("{id}: `Supersedes:` must be `none` or ADR-NNNN, got `{item}`"));
                assert!(numbers.contains(&n) && n != r.number, "{id}: supersedes {item}, which doesn't exist");
            }
        }
        for s in SECTIONS {
            assert!(r.text.lines().any(|l| l.starts_with(s)), "{id}: missing section `{s}`");
        }
    }
}

#[test]
fn index_matches_the_records() {
    let index = fs::read_to_string(root().join("docs/adr/index.md")).expect("read index.md");
    let mut rows: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
    for line in index.lines().filter(|l| l.trim_start().trim_start_matches('|').trim_start().starts_with("ADR-")) {
        // | ADR-NNNN | decision | status | decided | superseded by |
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        assert!(cells.len() == 7, "index.md: malformed row (want 5 columns): {line}");
        let n = adr_ref(cells[1]).unwrap_or_else(|| panic!("index.md: bad record number `{}`", cells[1]));
        let row = (cells[3].to_string(), cells[4].to_string(), cells[5].to_string());
        assert!(rows.insert(n, row).is_none(), "ADR-{n:04} listed twice in index.md");
    }
    for r in records() {
        let id = format!("ADR-{:04}", r.number);
        let (status, date, superseded_by) = rows.remove(&r.number).unwrap_or_else(|| panic!("{id} missing from index.md"));
        let rec_status = header(&r.text, "Status:").unwrap();
        assert_eq!(status, rec_status, "{id}: index status differs from the record");
        assert_eq!(date, header(&r.text, "Date decided:").unwrap(), "{id}: index date differs from the record");
        let want = rec_status.strip_prefix("Superseded by ").unwrap_or("—");
        assert_eq!(superseded_by, want, "{id}: index `Superseded by` differs from the record's status");
    }
    assert!(rows.is_empty(), "index.md lists records that don't exist: {:?}", rows.keys().collect::<Vec<_>>());
}

/// Whether `file` declares `fn name(` on a code line (not a comment) under a `#[test]` attribute.
fn is_test_fn(file: &str, name: &str) -> bool {
    let lines: Vec<&str> = file.lines().collect();
    lines.iter().enumerate().any(|(i, l)| {
        let t = l.trim_start();
        (t.starts_with(&format!("fn {name}(")) || t.starts_with(&format!("pub fn {name}(")))
            && lines[..i].iter().rev().map(|p| p.trim()).take_while(|p| p.starts_with("#[") || p.starts_with("///") || p.is_empty()).any(|p| p == "#[test]")
    })
}

/// Split a `Conformance:` value on ", " — but not inside a quoted CI job name (which may contain one).
fn split_entries(conf: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    let chars: Vec<char> = conf.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted && c == ',' && chars.get(i + 1) == Some(&' ') {
            out.push(std::mem::take(&mut cur));
            i += 2;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    out.push(cur);
    out
}

#[test]
fn conformance_entries_resolve() {
    for r in records() {
        let id = format!("ADR-{:04}", r.number);
        let conf = header(&r.text, "Conformance:").unwrap();
        if conf.starts_with("none — ") {
            continue;
        }
        for item in split_entries(conf) {
            let item = item.as_str();
            if let Some((path, test)) = item.split_once("::") {
                let file = fs::read_to_string(root().join(path)).unwrap_or_else(|_| panic!("{id}: Conformance cites `{path}`, which doesn't exist"));
                assert!(is_test_fn(&file, test), "{id}: Conformance cites `{test}`, which isn't a #[test] fn in {path}");
            } else if let Some((path, rest)) = item.split_once(" job \"") {
                let job = rest.split('"').next().unwrap_or_default();
                let file = fs::read_to_string(root().join(path)).unwrap_or_else(|_| panic!("{id}: Conformance cites `{path}`, which doesn't exist"));
                let named = file.lines().map(str::trim).any(|l| l.strip_prefix("name:").is_some_and(|n| n.trim().trim_matches('"') == job));
                assert!(named, "{id}: Conformance cites CI job `{job}`, which isn't a `name:` in {path}");
            } else {
                panic!("{id}: Conformance entry `{item}` must be `path::test`, `<workflow> job \"<name>\"`, or `none — <why>`");
            }
        }
    }
}
