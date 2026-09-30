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

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

/// `content` without `<!-- … -->` blocks, so a commented-out rule doesn't count.
fn xml_without_comments(content: &str) -> String {
    let mut out = String::new();
    let mut rest = content;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = rest[start..].find("-->").map_or("", |end| &rest[start + end + 3..]);
    }
    out + rest
}

/// ADR-0037: the scaffold's backup rules keep `securestore`'s file out of every backup and
/// device transfer, because its Keystore key never travels with it.
#[test]
fn adr_0037_template_backups_exclude_the_secure_store() {
    const EXCLUDE: &str = r#"<exclude domain="sharedpref" path="mobiler_secure.xml"/>"#;
    let xml = "mobiler/templates/Android/app/src/main/res/xml";
    let legacy = xml_without_comments(&read(&format!("{xml}/backup_rules.xml")));
    assert!(legacy.contains(EXCLUDE), "ADR-0037: backup_rules.xml must exclude mobiler_secure.xml");
    let rules = xml_without_comments(&read(&format!("{xml}/data_extraction_rules.xml")));
    for section in ["cloud-backup", "device-transfer"] {
        let open = format!("<{section}>");
        let body = rules
            .split_once(&open)
            .and_then(|(_, rest)| rest.split_once(&format!("</{section}>")))
            .map(|(body, _)| body)
            .unwrap_or_else(|| panic!("ADR-0037: data_extraction_rules.xml has no <{section}>"));
        assert!(body.contains(EXCLUDE), "ADR-0037: <{section}> must exclude mobiler_secure.xml");
    }
    let manifest = read("mobiler/templates/Android/app/src/main/AndroidManifest.xml");
    for attr in [r#"android:dataExtractionRules="@xml/data_extraction_rules""#, r#"android:fullBackupContent="@xml/backup_rules""#] {
        assert!(manifest.contains(attr), "ADR-0037: the manifest must point at the rules ({attr})");
    }
}

/// ADR-0038: plain HTTP is allowed only in debug builds, and only to the local dev hosts. The
/// main manifest (release builds) never opts into cleartext.
#[test]
fn adr_0038_cleartext_is_debug_only_and_local() {
    let main = xml_without_comments(&read("mobiler/templates/Android/app/src/main/AndroidManifest.xml"));
    for attr in ["usesCleartextTraffic", "networkSecurityConfig"] {
        assert!(!main.contains(attr), "ADR-0038: the main manifest must not set {attr}");
    }
    let debug = xml_without_comments(&read("mobiler/templates/Android/app/src/debug/AndroidManifest.xml"));
    assert!(debug.contains(r#"android:networkSecurityConfig="@xml/network_security_config""#));
    let config = xml_without_comments(&read("mobiler/templates/Android/app/src/debug/res/xml/network_security_config.xml"));
    assert!(!config.contains("<base-config"), "ADR-0038: no app-wide cleartext, even in debug");
    let domains: Vec<&str> = config
        .split("<domain ")
        .skip(1)
        .filter_map(|d| d.split_once('>').and_then(|(_, rest)| rest.split_once("</domain>")).map(|(host, _)| host.trim()))
        .collect();
    assert_eq!(domains, ["10.0.2.2", "localhost", "127.0.0.1"], "ADR-0038: cleartext only to the local dev hosts");
}

/// ADR-0039: the Android minimum is API 26 (Android 8.0), in both gradle modules of the template
/// and of every demo.
#[test]
fn adr_0039_android_minimum_is_api_26() {
    let shells = ["mobiler/templates/Android", "demos/barbershop/Android", "demos/coffee/Android", "demos/saldo/Android", "demos/todo/Android", "demos/fullstack-todo/mobile/Android"];
    for (shell, module) in shells.iter().flat_map(|s| ["app", "shared"].map(|m| (s, m))) {
        let gradle = read(&format!("{shell}/{module}/build.gradle.kts"));
        let min: Vec<&str> = gradle
            .lines()
            .filter_map(|l| l.trim().strip_prefix("minSdk = "))
            .collect();
        assert_eq!(min, ["26"], "ADR-0039: {shell}/{module}/build.gradle.kts must set minSdk = 26");
    }
}
