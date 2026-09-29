//! Keeps `docs/adr/` well-formed: the rules in `docs/adr/README.md` that a machine can check.
//!
//! - Files are `ADR-NNNN-slug.md`, numbered from 0001 with no gaps or duplicates, and each title
//!   carries its own number.
//! - Every record has the header fields (`Status`, `Date decided` as `YYYY-MM-DD`, `Deciding PRs`,
//!   `Supersedes`, `Code anchor`, `Conformance`) and the five numbered sections.
//! - `Status` uses the vocabulary; `Superseded by ADR-NNNN` names an existing record.
//! - `index.md` has exactly one row per record, with the same status and date.
//! - Every file a `Conformance:` entry cites exists.
//!
//! Mutation proof (this guard belongs to the ADR set as a whole, so its proof lives here):
//! - Renaming `ADR-0010-…` to `ADR-0011-…` failed `numbering_is_contiguous_from_one` ("gap or
//!   duplicate: expected ADR-0010, found ADR-0011-app-constraints-are-opt-in-plugins.md").
//! - Deleting the ADR-0007 row from `index.md` failed `index_matches_the_records` ("ADR-0007
//!   missing from index.md").
//! - Changing ADR-0005's `Status:` to `Approved` failed `every_record_is_well_formed`
//!   ("ADR-0005: unknown status `Approved`").
//! Each was reverted and the suite went green again.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

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

fn header<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| l.strip_prefix(key)).map(str::trim)
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 10 && b[4] == b'-' && b[7] == b'-' && s.chars().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
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
        assert!(is_date(header(&r.text, "Date decided:").unwrap()), "{id}: `Date decided:` must be YYYY-MM-DD");
        let status = header(&r.text, "Status:").unwrap();
        let ok = match status {
            "Accepted" | "Accepted (blocked)" | "Deprecated" => true,
            s => s
                .strip_prefix("Superseded by ADR-")
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| numbers.contains(&n) && n != r.number),
        };
        assert!(ok, "{id}: unknown status `{status}`");
        for s in SECTIONS {
            assert!(r.text.lines().any(|l| l.starts_with(s)), "{id}: missing section `{s}`");
        }
    }
}

#[test]
fn index_matches_the_records() {
    let index = fs::read_to_string(root().join("docs/adr/index.md")).expect("read index.md");
    let mut rows: BTreeMap<u32, (String, String)> = BTreeMap::new();
    for line in index.lines().filter(|l| l.starts_with("| ADR-")) {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // | ADR-NNNN | decision | status | decided | superseded by |
        let n: u32 = cells[1].trim_start_matches("ADR-").parse().expect("index number");
        assert!(rows.insert(n, (cells[3].to_string(), cells[4].to_string())).is_none(), "ADR-{n:04} listed twice in index.md");
    }
    for r in records() {
        let id = format!("ADR-{:04}", r.number);
        let (status, date) = rows.remove(&r.number).unwrap_or_else(|| panic!("{id} missing from index.md"));
        assert_eq!(status, header(&r.text, "Status:").unwrap(), "{id}: index status differs from the record");
        assert_eq!(date, header(&r.text, "Date decided:").unwrap(), "{id}: index date differs from the record");
    }
    assert!(rows.is_empty(), "index.md lists records that don't exist: {:?}", rows.keys().collect::<Vec<_>>());
}

#[test]
fn conformance_paths_exist() {
    for r in records() {
        let conf = header(&r.text, "Conformance:").unwrap();
        if conf.starts_with("none") {
            continue;
        }
        for item in conf.split(", ") {
            let path = item.split(|c: char| c == ' ').next().unwrap().split("::").next().unwrap();
            if path.contains('/') {
                assert!(Path::new(&root().join(path)).exists(), "ADR-{:04}: Conformance cites `{path}`, which doesn't exist", r.number);
            }
        }
    }
}
