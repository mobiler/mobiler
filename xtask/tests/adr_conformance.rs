//! Invariant tests for decisions in `docs/adr/` that a machine can check. Each test names its
//! record, and each record's §4 holds the test's mutation proof.

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("repo root").to_path_buf()
}

/// ADR-0003: `PluginResponse` stays exactly `{ ok, output }`; richer results ride inside `output`.
/// Checked on the facet shape, which is what the Kotlin/Swift types are generated from: a field
/// hidden from serde (`skip`, `skip_serializing_if`) would still widen the shells' ABI.
#[test]
fn adr_0003_plugin_response_has_exactly_ok_and_output() {
    use facet::{Facet, Type, UserType};
    let Type::User(UserType::Struct(st)) = <mobiler_core::PluginResponse as Facet>::SHAPE.ty else {
        panic!("ADR-0003: PluginResponse is no longer a struct");
    };
    let fields: Vec<(&str, String)> = st.fields.iter().map(|f| (f.name, f.shape().to_string())).collect();
    let names: Vec<&str> = fields.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["ok", "output"], "ADR-0003: PluginResponse gained or lost a field");
    assert_eq!(fields[0].1, "bool", "ADR-0003: `ok` changed type");
    assert!(fields[1].1.starts_with("Vec<u8"), "ADR-0003: `output` changed type: {}", fields[1].1);
}

/// Normal (non-dev, non-build) dependencies named `bincode` — by package name, so a renamed or
/// target-specific declaration counts too — for every package in the given workspace manifest.
fn bincode_normal_deps(manifest: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps", "--manifest-path"])
        .arg(root().join(manifest))
        .output()
        .expect("run cargo metadata");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse cargo metadata");
    let mut hits = Vec::new();
    for pkg in meta["packages"].as_array().expect("packages") {
        let name = pkg["name"].as_str().unwrap_or_default();
        if !["mobiler-ui", "mobiler-core", "mobiler-web"].contains(&name) {
            continue;
        }
        for dep in pkg["dependencies"].as_array().expect("dependencies") {
            if dep["name"] == "bincode" && dep["kind"].is_null() {
                hits.push(name.to_string());
            }
        }
    }
    hits
}

/// ADR-0004: payloads encode only through crux's `BincodeFfiFormat`, so no library crate may depend
/// on `bincode` itself (a direct 2.x call is varint, which the generated decoders can't read).
#[test]
fn adr_0004_no_library_crate_depends_on_bincode_directly() {
    // mobiler-ui and mobiler-core are root-workspace members; mobiler-web is its own workspace.
    let mut hits = bincode_normal_deps("Cargo.toml");
    hits.extend(bincode_normal_deps("mobiler-web/Cargo.toml"));
    assert!(hits.is_empty(), "ADR-0004: depends on bincode directly: {hits:?}");
}
