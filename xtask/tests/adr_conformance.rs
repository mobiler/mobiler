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
    // Every Android shell: the template's, and any demo's (found, so a new demo can't escape).
    let mut shells = vec!["mobiler/templates/Android".to_string()];
    for demo in std::fs::read_dir(root().join("demos")).expect("demos/").flatten() {
        for android in [demo.path().join("Android"), demo.path().join("mobile/Android")] {
            if android.join("app/build.gradle.kts").exists() {
                shells.push(android.strip_prefix(root()).unwrap().display().to_string());
            }
        }
    }
    assert!(shells.len() >= 6, "ADR-0039: expected the template and the demos' shells, found {shells:?}");
    for (shell, module) in shells.iter().flat_map(|s| ["app", "shared"].map(|m| (s, m))) {
        let gradle = read(&format!("{shell}/{module}/build.gradle.kts"));
        let min: Vec<&str> = gradle
            .lines()
            .filter_map(|l| l.trim().strip_prefix("minSdk = "))
            .collect();
        assert_eq!(min, ["26"], "ADR-0039: {shell}/{module}/build.gradle.kts must set minSdk = 26");
    }
}

/// ADR-0040: the Android field debug log never runs in a release build, and never logs a SECURE
/// field's text. Checked on every Android shell that has the log: the template's and any demo's.
#[test]
fn adr_0040_field_log_is_debug_only_and_never_logs_secure_text() {
    let mut shells = vec![root().join("mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt")];
    shells.extend(files_with_extension(&[root().join("demos")], "kt").into_iter().filter(|p| p.ends_with("MainActivity.kt")));
    let mut checked = 0;
    for path in &shells {
        let kt = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if !kt.contains("\"MobilerField\"") {
            continue;
        }
        checked += 1;
        let at = path.strip_prefix(root()).unwrap().display();

        // Only two lines name the tag: the gate, and the one write, behind it.
        for line in kt.lines().filter(|l| l.contains("\"MobilerField\"")) {
            let line = line.trim();
            assert!(
                line.starts_with("on = debuggable && android.util.Log.isLoggable(\"MobilerField\"")
                    || line == "if (on) android.util.Log.d(\"MobilerField\", msg())",
                "ADR-0040: {at}: the MobilerField tag is used outside the gate and the gated write: {line}"
            );
        }
        assert!(
            kt.contains("val debuggable = (info.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0"),
            "ADR-0040: {at}: the field log must be gated on the app being debuggable"
        );
        assert!(kt.contains("values = on && "), "ADR-0040: {at}: raw values need the log itself on");
        assert!(kt.contains("FieldLog.init(applicationInfo)"), "ADR-0040: {at}: onCreate must initialise the gate");

        // `show` answers SECURE before any branch that could print the text.
        let show = kt.split_once("fun show(text: String?, secure: Boolean)").map(|(_, b)| b).unwrap_or_else(|| panic!("ADR-0040: {at}: no FieldLog.show"));
        let show = &show[..show.find("\n    }").unwrap_or(show.len())];
        let secure = show.find("secure -> \"<secure len=${text.length}>\"").unwrap_or_else(|| panic!("ADR-0040: {at}: show must log a SECURE value as its length only"));
        let values = show.find("values ->").unwrap_or_else(|| panic!("ADR-0040: {at}: show has no values branch"));
        assert!(secure < values, "ADR-0040: {at}: show must check secure before printing raw values");

        // Every log message interpolates a field's text only through `show`.
        let messages: Vec<&str> = kt.lines().filter(|l| l.contains("FieldLog.d {")).collect();
        assert!(!messages.is_empty(), "ADR-0040: {at}: no FieldLog.d messages found");
        for line in messages {
            for (i, _) in line.match_indices('$') {
                let rest = &line[i + 1..];
                let ok = rest.starts_with("{FieldLog.show(")
                    || rest.split_once('}').is_some_and(|(expr, _)| {
                        expr.strip_prefix('{').and_then(|e| e.strip_suffix(".composition")).is_some_and(|v| v.chars().all(|c| c.is_alphanumeric()))
                    });
                assert!(ok, "ADR-0040: {at}: a field log message interpolates text without FieldLog.show: {}", line.trim());
            }
        }

        // A TextField tells its FieldSync its kind before the field is drawn.
        let text_field = kt.split_once("is Widget.TextField ->").map(|(_, b)| b).unwrap_or_else(|| panic!("ADR-0040: {at}: no TextField branch"));
        let before_draw = &text_field[..text_field.find("OutlinedTextField(").expect("TextField draws an OutlinedTextField")];
        assert!(before_draw.contains("sync.noteKind(widget.kind)"), "ADR-0040: {at}: the TextField must note its kind on its FieldSync");
    }
    assert!(checked >= 2, "ADR-0040: expected the template's and barbershop's field log, checked {checked}");
}

/// Every file with extension `ext` under `dirs`, skipping build output. Symlinks are not followed,
/// so a link loop can't hang the walk and a link can't lead outside the repo.
fn files_with_extension(dirs: &[PathBuf], ext: &str) -> Vec<PathBuf> {
    let mut stack = dirs.to_vec();
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten() {
            let (path, Ok(kind)) = (entry.path(), entry.file_type()) else { continue };
            if kind.is_dir() && !["build", "target", "node_modules", ".gradle"].iter().any(|s| path.ends_with(s)) {
                stack.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
    }
    out
}

/// The part of a Kotlin line that is code: a whole-line comment (`//`, `/*`, `*`) is not; a `//`
/// later in the line may sit inside a string (`"https://…"`), so the rest of the line counts.
fn kotlin_code(line: &str) -> &str {
    let t = line.trim_start();
    if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') { "" } else { line }
}

#[test]
fn walk_skips_symlinks_and_terminates() {
    let dir = std::env::temp_dir().join(format!("adr-walk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/a.kt"), "x").unwrap();
    std::os::unix::fs::symlink("..", dir.join("sub/loop")).unwrap();
    let found = files_with_extension(&[dir.clone()], "kt");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found, [dir.join("sub/a.kt")]);
}

#[test]
fn kotlin_code_keeps_a_call_after_a_url() {
    assert!(kotlin_code("val u = \"https://x\"; AppCompatDelegate.setDefaultNightMode(1)").contains("setDefaultNightMode"));
    assert!(!kotlin_code("    // AppCompatDelegate.setDefaultNightMode(1)").contains("setDefaultNightMode"));
    assert!(!kotlin_code("     * setDefaultNightMode is not called (ADR-0041)").contains("setDefaultNightMode"));
}

/// ADR-0041: no shell sets an app-level night mode, so the configuration the shell reads the OS
/// appearance from stays the OS's. Checked over every Kotlin source of the template and the demos.
#[test]
fn adr_0041_no_shell_sets_an_app_level_night_mode() {
    let files = files_with_extension(&[root().join("mobiler/templates/Android"), root().join("demos")], "kt");
    for path in &files {
        let kt = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for (i, line) in kt.lines().enumerate() {
            for call in ["setApplicationNightMode", "setDefaultNightMode"] {
                assert!(!kotlin_code(line).contains(call), "ADR-0041: {}:{} calls {call}", path.strip_prefix(root()).unwrap().display(), i + 1);
            }
        }
    }
    assert!(files.len() > 20, "ADR-0041: expected the shells' Kotlin sources, checked {}", files.len());
}

/// ADR-0047: the reference shell and the template render a container's children under a stable
/// identity (`childKeys`: the widget's id, else its kind plus the first id inside it, numbered among
/// siblings with the same base), never
/// by position, so a widget appearing above a field doesn't rebuild the field (focus, text) and a
/// player or map keeps its state.
#[test]
fn adr_0047_shells_key_children_by_identity_not_position() {
    let kotlin = [
        "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt",
        "mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt",
    ];
    let swift = ["demos/barbershop/iOS/Sources/Render.swift", "mobiler/templates/iOS/Sources/Render.swift"];
    // A child list walked by position: `children.forEach`, `kids.drop(1).forEach`, `pinned.forEachIndexed`,
    // `children.chunked(...)` — unless that same line wraps each child in `key(...)` (the helper itself)
    // or walks `.indices` (the Grid chunks indices to look each child's key up).
    let positional = |code: &str| {
        ["children", "kids", "pinned"].iter().any(|list| {
            code.match_indices(list).any(|(at, _)| {
                let word_start = at == 0 || !code.as_bytes()[at - 1].is_ascii_alphanumeric();
                let rest = &code[at + list.len()..];
                word_start && (rest.contains(".forEach") || rest.contains(".chunked("))
            })
        }) && !code.contains("key(keys[") && !code.contains(".indices.chunked(")
    };
    for path in kotlin {
        let text = std::fs::read_to_string(root().join(path)).expect(path);
        assert!(text.contains("private fun childKeys("), "ADR-0047: {path} has no childKeys");
        for (n, line) in text.lines().enumerate() {
            let code = kotlin_code(line);
            assert!(!positional(code), "ADR-0047: {path}:{} walks children by position: {line}", n + 1);
            let lazy_items = code.contains("items(") || code.contains("itemsIndexed(");
            assert!(
                !(lazy_items && code.contains("children") && !code.contains("key =")),
                "ADR-0047: {path}:{} lazy list over children without a key: {line}",
                n + 1
            );
        }
    }
    for path in swift {
        let text = std::fs::read_to_string(root().join(path)).expect(path);
        assert!(text.contains("func childKeys("), "ADR-0047: {path} has no childKeys");
        // Every ForEach over a child list (`children`, `kids`, `bar`) goes through keyedChildren.
        for (n, line) in text.lines().enumerate() {
            let over_children = ["children", "kids", "bar"].iter().any(|list| {
                line.match_indices(list).any(|(at, _)| {
                    let before = line[..at].chars().last();
                    let after = line[at + list.len()..].chars().next();
                    !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric())
                })
            });
            if line.contains("ForEach(") && over_children {
                assert!(line.contains("keyedChildren("), "ADR-0047: {path}:{} renders children by position: {line}", n + 1);
            }
        }
    }
}
