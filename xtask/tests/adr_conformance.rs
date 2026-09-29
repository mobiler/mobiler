//! Invariant tests for decisions in `docs/adr/` that a machine can check. Each test names its
//! record, and each record's §4 holds the test's mutation proof.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("repo root").to_path_buf()
}

/// ADR-0003: `PluginResponse` stays exactly `{ ok, output }`; richer results ride inside `output`.
#[test]
fn adr_0003_plugin_response_has_exactly_ok_and_output() {
    let v = serde_json::to_value(mobiler_core::PluginResponse::text(true, "x")).expect("serialize");
    let mut keys: Vec<&str> = v.as_object().expect("an object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["ok", "output"], "ADR-0003: PluginResponse gained or lost a field");
}

/// ADR-0004: payloads encode only through crux's `BincodeFfiFormat`, so no library crate may depend
/// on `bincode` itself (a direct 2.x call is varint, which the generated decoders can't read).
#[test]
fn adr_0004_no_library_crate_depends_on_bincode_directly() {
    for krate in ["mobiler-ui", "mobiler-core", "mobiler-web"] {
        let manifest = fs::read_to_string(root().join(krate).join("Cargo.toml")).expect("read Cargo.toml");
        let direct = manifest.lines().map(str::trim).any(|l| l.starts_with("bincode") && l.contains('='));
        assert!(!direct, "ADR-0004: {krate}/Cargo.toml depends on bincode directly");
    }
}
