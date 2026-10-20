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

/// ADR-0061: app data, the SQLite database included, stays in the platform's backup and transfer on
/// Android and iOS: the template's and Saldo's rules exclude nothing but the secure store (ADR-0037), and no
/// shell or bundled plugin marks a file as excluded from iCloud backup. A finance app is not an exception.
#[test]
fn adr_0061_app_data_stays_in_backups() {
    for app in ["mobiler/templates", "demos/saldo"] {
        for file in ["backup_rules.xml", "data_extraction_rules.xml"] {
            let path = format!("{app}/Android/app/src/main/res/xml/{file}");
            let xml = xml_without_comments(&read(&path));
            // Each exclude tag on its own, up to its first `>` (a paired `<exclude …></exclude>` too).
            let excludes: Vec<&str> = xml.match_indices("<exclude").map(|(at, _)| &xml[at..xml[at..].find('>').map_or(xml.len(), |e| at + e + 1)]).collect();
            assert!(!excludes.is_empty(), "ADR-0061: {path} has no exclude at all (ADR-0037's secure store)");
            for ex in excludes {
                assert!(ex.contains("mobiler_secure.xml"), "ADR-0061: {path} excludes app data from backup: {ex}");
            }
            // An include rule backs up only what it names; encryption-only cloud backup skips devices without it.
            assert!(!xml.contains("<include"), "ADR-0061: {path} has an include rule, which drops everything it doesn't name");
            assert!(!xml.contains("disableIfNoEncryptionCapabilities=\"true\""), "ADR-0061: {path} turns cloud backup off on some devices");
        }
        let manifest = read(&format!("{app}/Android/app/src/main/AndroidManifest.xml"));
        assert!(manifest.contains(r#"android:allowBackup="true""#), "ADR-0061: {app}'s manifest must allow backup");
    }
    // The data stays where backups reach: no iCloud exclusion, no move to a never-backed-up folder.
    let swift = files_with_extension(&[root().join("mobiler/templates/iOS"), root().join("mobiler/plugins"), root().join("demos/saldo/iOS")], "swift");
    assert!(swift.len() > 10, "ADR-0061: expected the iOS sources, found {}", swift.len());
    for path in &swift {
        let code = live_code(&std::fs::read_to_string(path).unwrap());
        for key in ["isExcludedFromBackup", "NSURLIsExcludedFromBackupKey"] {
            assert!(!code.contains(key), "ADR-0061: {} excludes a file from iCloud backup ({key})", path.display());
        }
    }
    let kotlin = files_with_extension(&[root().join("mobiler/templates/Android"), root().join("mobiler/plugins"), root().join("demos/saldo/Android/app/src")], "kt");
    for path in &kotlin {
        assert!(!live_code(&std::fs::read_to_string(path).unwrap()).contains("noBackupFilesDir"), "ADR-0061: {} keeps data outside backup", path.display());
    }
    for path in ["mobiler/plugins/sqlite/ios/SqlitePlugin.swift", "demos/saldo/iOS/Sources/SqlitePlugin.swift"] {
        let code = live_code(&read(path));
        assert!(code.contains(".documentDirectory") && !code.contains("cachesDirectory") && !code.contains("temporaryDirectory"), "ADR-0061: {path} must keep mobiler.db in Documents (backed up)");
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

/// The body of the function whose signature starts at `sig`: from the first `{` after it to its
/// matching `}` (braces counted, so nested blocks stay inside).
fn fn_body<'a>(code: &'a str, sig: &str) -> Option<&'a str> {
    let open = code[code.find(sig)?..].find('{')? + code.find(sig)?;
    let mut depth = 0usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&code[open..=open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn fn_body_matches_nested_braces() {
    let code = "fun a() { if (x) { y() } else { z() } }\nfun b() { w() }";
    assert_eq!(fn_body(code, "fun a("), Some("{ if (x) { y() } else { z() } }"));
    assert_eq!(fn_body(code, "fun b("), Some("{ w() }"));
    assert_eq!(fn_body(code, "fun c("), None);
}

#[test]
fn adr_0057_android_keeps_tab_screens_measuring_only_the_active_one() {
    let kotlin = [
        "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt",
        "mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt",
    ];
    for path in kotlin {
        let text = std::fs::read_to_string(root().join(path)).expect(path);
        let code: String = text.lines().map(kotlin_code).collect::<Vec<_>>().join("\n");
        let host = fn_body(&code, "private fun TabSlots(").unwrap_or_else(|| panic!("ADR-0057: {path} has no TabSlots"));
        let measure = &host[host.find("measurables, constraints ->").unwrap_or_else(|| panic!("ADR-0057: {path} TabSlots has no measure policy"))..];
        assert!(
            measure.contains("measurables[shown]")
                && !["measurables.map", "measurables.forEach", "for ("].iter().any(|m| measure.contains(m)),
            "ADR-0057: {path} TabSlots must measure only the active slot"
        );
        assert!(
            host.contains("rememberUpdatedState(i == slot)") && host.contains("if (isActive) send("),
            "ADR-0057: {path} a hidden slot's send must be dropped"
        );
        assert!(host.contains("keptWhenHidden("), "ADR-0057: {path} TabSlots must ask keptWhenHidden before keeping a hidden screen");
        assert!(host.contains("clearFocus("), "ADR-0057: {path} hiding a slot must clear focus");
        let live = fn_body(&code, "private fun holdsLiveView(").unwrap_or_else(|| panic!("ADR-0057: {path} has no holdsLiveView"));
        for kind in ["Widget.WebView", "Widget.Video", "Widget.Map", "Widget.PdfView"] {
            assert!(live.contains(kind), "ADR-0057: {path} holdsLiveView misses {kind}");
        }
        // Not kept either: a screen still animating (refreshing, an indeterminate progress, a loading
        // list), a nested Scaffold (its BackHandler and sheet outlive placement), a SECURE field's text.
        let kept = fn_body(&code, "private fun keptWhenHidden(").unwrap_or_else(|| panic!("ADR-0057: {path} has no keptWhenHidden"));
        for rule in ["holdsLiveView(", ".refreshing", "Widget.Progress", ".loading", "Widget.Scaffold", "FieldKind.SECURE"] {
            assert!(kept.contains(rule), "ADR-0057: {path} keptWhenHidden misses {rule}");
        }
    }
}

/// ADR-0058: the iOS shell keeps visited tab screens by ADR-0057's rules. A hidden slot can't be tapped or
/// read, sends nothing, a switch ends editing, and only a screen `keptWhenHidden` allows is kept.
#[test]
fn adr_0058_ios_keeps_tab_screens_like_android() {
    let swift = ["demos/barbershop/iOS/Sources/Render.swift", "mobiler/templates/iOS/Sources/Render.swift"];
    for path in swift {
        let code = live_code(&std::fs::read_to_string(root().join(path)).expect(path));
        let host = fn_body(&code, "private struct TabSlots").unwrap_or_else(|| panic!("ADR-0058: {path} has no TabSlots"));
        // The slot cache decides what is kept; the host only renders it.
        let cache = fn_body(&code, "private final class SlotCache").unwrap_or_else(|| panic!("ADR-0058: {path} has no SlotCache"));
        assert!(cache.contains("keptWhenHidden("), "ADR-0058: {path} SlotCache must ask keptWhenHidden before keeping a hidden screen");
        assert!(host.contains("cache.update("), "ADR-0058: {path} TabSlots must render what SlotCache keeps");
        assert!(
            host.contains(".allowsHitTesting(") && host.contains(".accessibilityHidden("),
            "ADR-0058: {path} a hidden slot must not be tapped or read"
        );
        assert!(host.contains("resignFirstResponder"), "ADR-0058: {path} a switch must end editing");
        // A hardware keyboard or Full Keyboard Access moves focus without hit testing: a hidden slot is disabled.
        assert!(host.contains(".disabled(!shown)"), "ADR-0058: {path} a hidden slot's fields must not take keyboard focus");
        assert!(host.contains("if gate.active { send("), "ADR-0058: {path} a hidden slot's send must be dropped");
        let live = fn_body(&code, "func holdsLiveView(").unwrap_or_else(|| panic!("ADR-0058: {path} has no holdsLiveView"));
        for kind in [".webView", ".video", ".map", ".pdfView"] {
            assert!(live.contains(kind), "ADR-0058: {path} holdsLiveView misses {kind}");
        }
        let kept = fn_body(&code, "func keptWhenHidden(").unwrap_or_else(|| panic!("ADR-0058: {path} has no keptWhenHidden"));
        for rule in ["holdsLiveView(", "refreshing", ".progress", ".lazyList", ".scaffold", ".secure"] {
            assert!(kept.contains(rule), "ADR-0058: {path} keptWhenHidden misses {rule}");
        }
    }
}

/// ADR-0059: an option only one platform renders is named for it, documented as such, read by that
/// platform's shell and ignored by every other: `ios_glass_tabs` is read by the iOS shells, never by an
/// Android shell, and bound only as ignored by the web shell.
#[test]
fn adr_0059_platform_only_option_is_named_and_ignored_elsewhere() {
    let ui = read("mobiler-ui/src/lib.rs");
    let at = ui.find("ios_glass_tabs: bool").expect("ADR-0059: Scaffold has no ios_glass_tabs");
    let doc = &ui[ui[..at].rfind("bottom_bar: Option<Vec<Widget>>,").expect("bottom_bar")..at];
    assert!(doc.contains("iOS only"), "ADR-0059: ios_glass_tabs' doc must say it is iOS only");
    for path in ["demos/barbershop/iOS/Sources/Render.swift", "mobiler/templates/iOS/Sources/Render.swift"] {
        let code = live_code(&read(path));
        assert!(code.contains("iosGlassTabs"), "ADR-0059: {path} must render ios_glass_tabs");
        // The capsule grows with Dynamic Type: what it covers is measured, never a fixed height.
        assert!(
            code.contains("onGeometryChange(") && !code.contains("glassTabClearance"),
            "ADR-0059: {path} must clear the glass capsule by its measured height"
        );
    }
    // Shell code only: generated bindings and build output name every field.
    let kotlin: Vec<PathBuf> = files_with_extension(&[root().join("mobiler/templates/Android"), root().join("demos")], "kt")
        .into_iter()
        .filter(|p| !p.components().any(|c| matches!(c.as_os_str().to_str(), Some("generated" | "build"))))
        .collect();
    assert!(kotlin.len() > 20, "ADR-0059: expected the shells' Kotlin sources, found {}", kotlin.len());
    for path in &kotlin {
        let kt = std::fs::read_to_string(path).unwrap();
        assert!(!live_code(&kt).contains("iosGlassTabs"), "ADR-0059: {} reads the iOS-only option", path.display());
    }
    let web = live_code(&read("mobiler-web/src/lib.rs"));
    assert_eq!(web.matches("ios_glass_tabs").count(), web.matches("ios_glass_tabs: _").count(), "ADR-0059: the web shell reads the iOS-only option");
}

/// ADR-0019: every palette colour role is drawn by both native shells (barbershop, the reference,
/// and the template): each `ColorRoles` field, in its generated camelCase name, is read by the Android
/// shell (MainActivity.kt or ui/theme/Theme.kt) and the iOS shell (Render.swift or Core.swift).
#[test]
fn every_palette_role_is_read_by_both_native_shells() {
    let ui = read("mobiler-ui/src/lib.rs");
    let body = fn_body(&ui, "pub struct ColorRoles").expect("ColorRoles");
    let roles: Vec<String> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub ")?.split(':').next().map(str::to_string))
        .map(|snake| {
            let mut out = String::new();
            let mut up = false;
            for c in snake.chars() {
                if c == '_' { up = true } else if up { out.extend(c.to_uppercase()); up = false } else { out.push(c) }
            }
            out
        })
        .collect();
    assert!(roles.len() >= 28, "expected the ColorRoles fields, found {roles:?}");
    let shells = [
        ("demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/", vec!["MainActivity.kt", "ui/theme/Theme.kt"]),
        ("mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/", vec!["MainActivity.kt", "ui/theme/Theme.kt"]),
        ("demos/barbershop/iOS/Sources/", vec!["Render.swift", "Core.swift"]),
        ("mobiler/templates/iOS/Sources/", vec!["Render.swift", "Core.swift"]),
    ];
    for (dir, files) in shells {
        let code: String = files.iter().map(|f| live_code(&read(&format!("{dir}{f}")))).collect();
        for role in &roles {
            // A palette read: the role is optional, so it is reached with `?.role`, read as `.role?`, or
            // read off the palette value itself (`p.role`); Material's `colorScheme.outline` and SwiftUI's
            // `Color.primary` are none of these. The name ends there (`.outline` ≠ `.outlinedFill`).
            let read = code.match_indices(&format!(".{role}")).any(|(at, m)| {
                let next = code[at + m.len()..].chars().next();
                let ends = !next.is_some_and(|c| c.is_alphanumeric() || c == '_');
                let owner = code[..at].rsplit(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
                ends && (code[..at].ends_with('?') || next == Some('?') || ["p", "pal", "roles", "palette"].contains(&owner))
            });
            assert!(read, "ADR-0019: {dir} never reads the palette role {role}");
        }
    }
}

/// Not an ADR: the core capabilities' Android permissions are declared by the template, so an app gets
/// `cx.http` and `cx.haptic` without a hand edit (which a CLI-managed demo may not carry). Both are normal
/// permissions: granted at install, no prompt.
#[test]
fn the_template_declares_the_core_capabilities_permissions() {
    let manifest = xml_without_comments(&read("mobiler/templates/Android/app/src/main/AndroidManifest.xml"));
    for perm in ["android.permission.INTERNET", "android.permission.VIBRATE"] {
        assert!(manifest.contains(&format!(r#"<uses-permission android:name="{perm}" />"#)), "the template must declare {perm}");
    }
}

/// Not an ADR: the CLI embeds `mobiler/templates` and `mobiler/plugins` whole, so a leftover from a
/// `patch` run or a review copy would ship in every new app (a `MainActivity.kt.orig` once did, #305).
#[test]
fn templates_and_plugins_hold_no_leftover_files() {
    let mut dirs = vec![root().join("mobiler/templates"), root().join("mobiler/plugins")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                ![".orig", ".rej", ".mobiler-new", ".mobiler-bak", ".swp", "~"].iter().any(|e| name.ends_with(e)),
                "{} is a leftover, not a template file",
                path.display()
            );
        }
    }
}

/// Not an ADR: the bug fixed beside ADR-0057 (#302). A theme that builds a new `ColorScheme` on every core
/// update recomposes the whole app on every update, so every shell's theme builds it once per distinct theme.
#[test]
fn every_android_theme_builds_its_color_scheme_once() {
    let themes: Vec<PathBuf> = files_with_extension(&[root().join("mobiler/templates/Android"), root().join("demos")], "kt")
        .into_iter()
        .filter(|p| p.ends_with("ui/theme/Theme.kt"))
        .collect();
    for path in &themes {
        let code: String = std::fs::read_to_string(path).unwrap().lines().map(kotlin_code).collect::<Vec<_>>().join("\n");
        let rel = path.strip_prefix(root()).unwrap().display();
        assert!(code.contains("val colorScheme = remember("), "{rel} builds a new ColorScheme on every update");
        assert!(code.contains("val typography = remember("), "{rel} builds new typography on every update");
        assert!(code.contains("val shapes = remember("), "{rel} builds new shapes on every update");
    }
    assert!(themes.len() >= 6, "expected the template's and every demo's Theme.kt, found {}", themes.len());
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

/// ADR-0048: the `[splash]` sync fills marker blocks in the template's framework files (never inserts
/// them), and every app-owned file it writes is an upgrade seed (ADR-0042), so `upgrade` never merges
/// or overwrites what the sync wrote.
#[test]
fn adr_0048_splash_markers_and_seeds() {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).expect(p);
    for theme in ["values-v31", "values-night-v31"] {
        let path = format!("mobiler/templates/Android/app/src/main/res/{theme}/mobiler_themes.xml");
        let text = read(&path);
        assert_eq!(text.matches("<!-- mobiler:splash-begin -->").count(), 1, "ADR-0048: {path} splash begin marker");
        assert_eq!(text.matches("<!-- mobiler:splash-end -->").count(), 1, "ADR-0048: {path} splash end marker");
        let style_end = text.find("</style>").expect("a style");
        assert!(text.find("mobiler:splash-end").unwrap() < style_end, "ADR-0048: {path} markers must sit inside the theme");
    }
    let yml = read("mobiler/templates/iOS/project.yml");
    let launch = yml.find("UILaunchScreen:").expect("UILaunchScreen in project.yml");
    let begin = yml.find("# mobiler:splash-begin").expect("ADR-0048: project.yml splash begin marker");
    let end = yml.find("# mobiler:splash-end").expect("ADR-0048: project.yml splash end marker");
    assert!(launch < begin && begin < end, "ADR-0048: project.yml markers must sit under UILaunchScreen");
    let upgrade = read("mobiler/src/upgrade.rs");
    let seeds = &upgrade[upgrade.find("const SEED_PATHS").expect("SEED_PATHS")..];
    let seeds = &seeds[..seeds.find("];").expect("end of SEED_PATHS")];
    for written in [
        "Android/app/src/main/res/values/mobiler_splash.xml",
        "Android/app/src/main/res/values-night/mobiler_splash.xml",
        "Android/app/src/main/res/drawable/mobiler_launch.xml",
        "iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json",
    ] {
        assert!(seeds.contains(&format!("\"{written}\"")), "ADR-0048: {written} is written by the sync but isn't a seed");
    }
}

/// ADR-0050: every shell draws a tab's badge in both the bottom bar and the wide-screen rail, through its one badge
/// helper, so neither layout can lose it.
#[test]
fn adr_0050_every_tab_bar_and_rail_draws_the_badge() {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).expect(p);
    // The text between `start` and the first `end` after it.
    let region = |text: &str, start: &str, end: &str, path: &str| -> String {
        let at = text.find(start).unwrap_or_else(|| panic!("ADR-0050: {path} has no {start}"));
        let rest = &text[at..];
        rest[..rest.find(end).unwrap_or_else(|| panic!("ADR-0050: {path} has no {end} after {start}"))].to_string()
    };
    let kotlin = [
        "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt",
        "mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt",
    ];
    for path in kotlin {
        let text = read(path);
        for item in ["NavigationRailItem(", "NavigationBarItem("] {
            let args = region(&text, item, "colors =", path);
            assert!(args.contains("TabIcon(t)") && args.contains("TabLabel(t)"), "ADR-0050: {path} {item} doesn't draw the badge");
        }
    }
    let swift = ["demos/barbershop/iOS/Sources/Render.swift", "mobiler/templates/iOS/Sources/Render.swift"];
    for path in swift {
        let text = read(path);
        let bar = region(&text, "if showBottomTabs && !tabs.isEmpty", "private var navRail", path);
        let rail = region(&text, "private var navRail", "private var navTransition", path);
        for (name, part) in [("bottom bar", bar), ("rail", rail)] {
            assert!(part.contains("tabBadgeMark(") && part.contains("tabSpoken(tab)"), "ADR-0050: {path} {name} doesn't draw the badge");
        }
    }
    let web = read("mobiler-web/src/lib.rs");
    let tabbar = region(&web, "let tabbar =", "<div class=\"tabbar\">", "mobiler-web/src/lib.rs");
    assert!(tabbar.contains("tab_badge(tab.badge"), "ADR-0050: the web tabbar doesn't draw the badge");
}

/// ADR-0051: the deep-link registration sits between one pair of `mobiler:deep-links` markers in the
/// Android manifest and in `project.yml`, the template and barbershop alike, so `[deep_links] enabled =
/// false` can switch it off. The launcher filter stays outside, and the template has no other
/// BROWSABLE filter or URL type left on.
#[test]
fn adr_0051_deep_link_registration_sits_between_markers() {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).expect(p);
    // The text strictly between the one `begin` and the one `end` line, and the text outside it.
    let split = |text: &str, begin: &str, end: &str, path: &str| -> (String, String) {
        assert_eq!(text.matches(begin).count(), 1, "ADR-0051: {path} needs exactly one {begin}");
        assert_eq!(text.matches(end).count(), 1, "ADR-0051: {path} needs exactly one {end}");
        let (b, e) = (text.find(begin).unwrap(), text.find(end).unwrap());
        assert!(b < e, "ADR-0051: {path} markers are out of order");
        (text[b + begin.len()..e].to_string(), format!("{}{}", &text[..b], &text[e..]))
    };
    for (path, template) in [
        ("mobiler/templates/Android/app/src/main/AndroidManifest.xml", true),
        ("demos/barbershop/Android/app/src/main/AndroidManifest.xml", false),
    ] {
        let (inside, outside) = split(&read(path), "<!-- mobiler:deep-links-begin -->", "<!-- mobiler:deep-links-end -->", path);
        assert!(inside.contains("android.intent.category.BROWSABLE") && inside.contains("android:scheme="), "ADR-0051: {path} markers must wrap the deep-link filter");
        assert!(!inside.contains("LAUNCHER"), "ADR-0051: {path} markers must not wrap the launcher filter");
        assert!(!inside.contains("--"), "ADR-0051: {path} deep-link block can't be commented out (it holds `--`)");
        if template {
            assert!(!outside.contains("BROWSABLE"), "ADR-0051: {path} has a BROWSABLE filter outside the markers");
        }
    }
    for path in ["mobiler/templates/iOS/project.yml", "demos/barbershop/iOS/project.yml"] {
        let (inside, outside) = split(&read(path), "# mobiler:deep-links-begin", "# mobiler:deep-links-end", path);
        assert!(inside.contains("CFBundleURLTypes:") && inside.contains("CFBundleURLSchemes:"), "ADR-0051: {path} markers must wrap CFBundleURLTypes");
        assert!(!outside.contains("CFBundleURLTypes:"), "ADR-0051: {path} has a URL type outside the markers");
    }
}

/// ADR-0052: every shell answers `clipboard_with`, the capability `cx.copy_with` targets, so a copy with options never
/// falls back to the plain `clipboard` handler (which ignores its op and would copy a secret as-is). A commented-out
/// line doesn't count, and the templates' handler stays byte-identical to barbershop's, where it is tested first
/// (ADR-0015).
#[test]
fn adr_0052_every_shell_answers_clipboard_with() {
    let answers = |path: &str, needle: &str| live_code(&read(path)).contains(needle);
    let kotlin = "\"clipboard_with\" to ClipboardWithPlugin(";
    let swift = "case \"clipboard_with\": return await ClipboardWithPlugin.handle(";
    let (tk, ts) = ("mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt", "mobiler/templates/iOS/Sources/Core.swift");
    let (bk, bs) = ("demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt", "demos/barbershop/iOS/Sources/Core.swift");
    for (path, needle) in [(tk, kotlin), (ts, swift), (bk, kotlin), (bs, swift), ("mobiler-web/src/lib.rs", "call.plugin == \"clipboard_with\"")] {
        assert!(answers(path, needle), "ADR-0052: {path} doesn't answer clipboard_with");
    }
    // The handler block: from its doc comment up to the share plugin that follows it in every shell.
    let block = |path: &str, end: &str| {
        let s = read(path);
        let from = s.find("Built-in `clipboard_with` capability").unwrap_or_else(|| panic!("ADR-0052: {path} has no clipboard_with handler"));
        let to = from + s[from..].find(end).unwrap_or_else(|| panic!("ADR-0052: {path} has no {end} after the handler"));
        s[from..to].to_string()
    };
    assert!(block(tk, "class SharePlugin") == block(bk, "class SharePlugin"), "ADR-0052: {tk}'s clipboard_with handler differs from barbershop's");
    assert!(block(ts, "enum SharePlugin") == block(bs, "enum SharePlugin"), "ADR-0052: {ts}'s clipboard_with handler differs from barbershop's");
    let core = read("mobiler-core/src/lib.rs");
    assert!(core.contains("self.plugin(\"clipboard_with\", \"copy\""), "ADR-0052: cx.copy_with must target clipboard_with");
    assert!(core.contains("self.notify(\"clipboard\", \"copy\", text)"), "ADR-0007: cx.copy must stay a plain notification");
}

/// The live code in `src`: whole-line `//` comments, `/* … */` blocks (also across lines) and the dead branch of a
/// Swift `#if false` (up to its `#else` or `#endif`, nested `#if`s included) are dropped, as are the `#if` / `#else` /
/// `#endif` lines themselves, and so is any line starting with `*` (a doc-comment body; a live `*x = …` line would be
/// lost too, which can only make a check fail, never pass). Every other line is kept whole (a `//` later in a line may
/// sit inside a string). Any other `#if` condition counts as live on both branches; `#elseif` counts as `#else`. Not
/// handled: nested block comments and a `/*` opened mid-line, which the shells don't use around what is checked.
fn live_code(src: &str) -> String {
    // One frame per open `#if`: (inside a dead region already, is `#if false`, past its `#else`).
    let mut frames: Vec<(bool, bool, bool)> = Vec::new();
    let dead = |f: &[(bool, bool, bool)]| f.last().is_some_and(|&(outer, is_false, in_else)| outer || (is_false && !in_else));
    let (mut out, mut in_block) = (String::new(), false);
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("#if") {
            frames.push((dead(&frames), t.starts_with("#if false"), false));
            continue;
        }
        if t.starts_with("#else") {
            if let Some(top) = frames.last_mut() {
                top.2 = true;
            }
            continue;
        }
        if t.starts_with("#endif") {
            frames.pop();
            continue;
        }
        if dead(&frames) {
            continue;
        }
        if in_block {
            if let Some(end) = line.find("*/") {
                in_block = false;
                out.push_str(&line[end + 2..]);
                out.push('\n');
            }
            continue;
        }
        if t.starts_with("//") || t.starts_with('*') {
            continue;
        }
        if t.starts_with("/*") {
            match line.find("*/") {
                Some(end) => {
                    out.push_str(&line[end + 2..]);
                    out.push('\n');
                }
                None => in_block = true,
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn live_code_drops_comments_blocks_and_if_false() {
    let src = "a\n// b\n/* c\n d\n*/ e\n#if false\nf\n#if X\ng\n#endif\nh\n#endif\ni\n  * j\nk // l\n\
               #if false\nm\n#else\nn\n#endif\n#if os(iOS)\no\n#else\np\n#endif\n";
    assert_eq!(live_code(src), "a\n e\ni\nk // l\nn\no\np\n");
}

/// ADR-0053: every shell's `clipboard_with` success path builds the report, in live code, and the core can read it.
#[test]
fn adr_0053_every_shell_reports_what_a_copy_got() {
    for path in [
        "mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt",
        "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt",
    ] {
        let code = live_code(&read(path));
        for needle in [".put(\"sensitive\", sensitive)", ".put(\"expires\", ", ".put(\"local_only\", false)"] {
            assert!(code.contains(needle), "ADR-0053: {path} doesn't report {needle}");
        }
    }
    for path in ["mobiler/templates/iOS/Sources/Core.swift", "demos/barbershop/iOS/Sources/Core.swift"] {
        let code = live_code(&read(path));
        assert!(
            code.contains("[\"sensitive\": sensitive, \"expires\": expires, \"local_only\": localOnly]"),
            "ADR-0053: {path} doesn't report what a copy got"
        );
    }
    let web = live_code(&read("mobiler-web/src/lib.rs"));
    assert!(web.contains("Ok(_) => PluginResponse::text(true, WEB_COPY_REPORT)"), "ADR-0053: mobiler-web doesn't report");
    let core = read("mobiler-core/src/clipboard.rs");
    assert!(core.contains("pub fn from_response(resp: &crate::PluginResponse) -> Option<Self>"), "ADR-0053: core can't read the report");
}

/// Every `shared/src/bin/codegen.rs` (and its `shared/Cargo.toml`) under `demos/`.
fn demo_shared_crates() -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name();
            // Dot-dirs too: `.mobiler/base/` holds a snapshot of the template's shared/ files, not a crate.
            let name_str = name.to_str().unwrap_or("");
            if !e.file_type().is_ok_and(|t| t.is_dir()) || name_str.starts_with('.') || ["target", "node_modules", "build"].contains(&name_str) {
                continue;
            }
            if name == "shared" && p.join("src/bin/codegen.rs").is_file() {
                out.push(p.clone());
            }
            walk(&p, out);
        }
    }
    let mut out = Vec::new();
    walk(&root().join("demos"), &mut out);
    out.sort();
    out
}

/// The exact `uniffi = { version = "=X", … }` pin in a manifest.
fn uniffi_pin(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix("uniffi = { version = \"")?;
        rest.split('"').next().map(str::to_string)
    })
}

/// ADR-0054: the app's codegen makes the bindings through UniFFI, never through crux's bindgen, and
/// pins UniFFI to exactly the `uniffi_bindgen` version crux_core pins.
#[test]
fn adr_0054_codegen_uses_uniffi_not_crux_bindgen() {
    let template = root().join("mobiler/templates/shared");
    let mut crates = vec![template.clone()];
    crates.extend(demo_shared_crates());
    assert!(crates.len() >= 6, "ADR-0054: expected the template and five demos, found {crates:?}");

    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(root().join("Cargo.toml"))
        .output()
        .expect("run cargo metadata");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse cargo metadata");
    let crux_pin = meta["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter(|p| p["name"] == "crux_core")
        .flat_map(|p| p["dependencies"].as_array().cloned().unwrap_or_default())
        .find(|d| d["name"] == "uniffi_bindgen")
        .and_then(|d| d["req"].as_str().map(str::to_string))
        .expect("ADR-0054: crux_core no longer declares uniffi_bindgen");

    for dir in crates {
        let codegen = std::fs::read_to_string(dir.join("src/bin/codegen.rs")).expect("read codegen.rs");
        let live = live_code(&codegen);
        for banned in ["crux_core::cli", "crux_core::bindgen", "cli::{", "bindgen::{"] {
            assert!(!live.contains(banned), "ADR-0054: {} uses crux's bindgen ({banned})", dir.display());
        }
        assert!(live.contains("generate_bindings_library_mode"), "ADR-0054: {} doesn't call UniFFI's library mode", dir.display());
        let manifest_name = if dir == template { "Cargo.toml.tmpl" } else { "Cargo.toml" };
        let manifest = std::fs::read_to_string(dir.join(manifest_name)).expect("read shared manifest");
        assert!(!manifest.contains("\"crux_core/cli\""), "ADR-0054: {} enables crux_core/cli", dir.display());
        let pin = uniffi_pin(&manifest).unwrap_or_else(|| panic!("ADR-0054: {} has no exact uniffi pin", dir.display()));
        assert_eq!(pin, crux_pin, "ADR-0054: {} pins uniffi {pin}, crux_core pins uniffi_bindgen {crux_pin}", dir.display());
    }
}

/// A manifest's version for `dep`: `dep = "X"` or `dep = { version = "X", … }`.
fn string_pin(manifest: &str, dep: &str) -> Option<String> {
    let prefix = format!("{dep} = ");
    manifest.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix(&prefix)?;
        let rest = rest.strip_prefix('"').or_else(|| rest.strip_prefix("{ version = \""))?;
        rest.split('"').next().map(str::to_string)
    })
}

/// ADR-0055: the pins `mobiler upgrade` moves an app to are the ones the framework itself builds
/// with — the template's crux_core is the workspace's, and the facet version upgrade tells apps to
/// use is mobiler-core's.
#[test]
fn adr_0055_template_crux_pin_matches_the_workspace() {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).unwrap_or_else(|e| panic!("read {p}: {e}"));
    let template = string_pin(&read("mobiler/templates/Cargo.toml.tmpl"), "crux_core").expect("template pins crux_core");
    let workspace = string_pin(&read("Cargo.toml"), "crux_core").expect("workspace pins crux_core");
    assert_eq!(template, workspace, "ADR-0055: the template pins crux_core {template}, the workspace builds with {workspace}");

    let core_facet = string_pin(&read("mobiler-core/Cargo.toml"), "facet").expect("mobiler-core pins facet");
    let upgrade = read("mobiler/src/upgrade.rs");
    let told = upgrade
        .lines()
        .find_map(|l| l.strip_prefix("const FACET_PIN: &str = \"")?.split('"').next().map(str::to_string))
        .expect("ADR-0055: upgrade.rs has no FACET_PIN");
    assert_eq!(told, core_facet, "ADR-0055: upgrade tells apps facet {told}, mobiler-core builds with {core_facet}");
}

/// A plugin-insertion anchor: a comment line naming `mobiler:` (not a line that merely mentions it).
fn is_anchor(line: &str) -> bool {
    let t = line.trim_start();
    ["// mobiler:", "# mobiler:", "<!-- mobiler:"].iter().any(|a| t.starts_with(a))
}

/// Whether `app` is `template` plus only what `mobiler plugin add` inserts: blocks of extra lines, each
/// ending directly above an anchor comment both files have, and each line exactly one of `allowed` (a
/// used bundled plugin's rendered insertions: its registration, dependency, manifest and plist lines;
/// a trailing comma is the registration list's). `Err` names the first line that isn't.
fn plugin_insertions_only(template: &str, app: &str, allowed: &[String]) -> Result<(), String> {
    let (t, a): (Vec<&str>, Vec<&str>) = (template.lines().collect(), app.lines().collect());
    let (mut i, mut j) = (0, 0);
    while j < a.len() {
        if i < t.len() && a[j] == t[i] {
            (i, j) = (i + 1, j + 1);
            continue;
        }
        let start = j;
        while j < a.len() && (i >= t.len() || a[j] != t[i]) {
            j += 1;
        }
        if i >= t.len() || !is_anchor(t[i]) {
            return Err(format!("line {}: {}", start + 1, a[start]));
        }
        let inserted = |line: &str| {
            let line = line.trim();
            allowed.iter().any(|ok| line == ok.as_str() || line.strip_suffix(',') == Some(ok.as_str()))
        };
        if let Some(k) = (start..j).find(|&k| !inserted(a[k])) {
            return Err(format!("line {}: {} (no bundled plugin inserts it)", k + 1, a[k]));
        }
    }
    match t.get(i) {
        Some(missing) => Err(format!("missing template line {}: {missing}", i + 1)),
        None => Ok(()),
    }
}

#[test]
fn plugin_insertions_only_accepts_anchor_blocks_only() {
    let t = "a\nb\n// mobiler:plugins — insert above\nc";
    let ok = vec!["\"x\" to XPlugin()".to_string()];
    assert_eq!(plugin_insertions_only(t, t, &ok), Ok(()));
    assert_eq!(plugin_insertions_only(t, "a\nb\n    \"x\" to XPlugin(),\n// mobiler:plugins — insert above\nc", &ok), Ok(()));
    assert!(plugin_insertions_only(t, "a\nb\n    \"foo\" to FooPlugin(),\n// mobiler:plugins — insert above\nc", &ok).is_err(), "a hand-written registration above an anchor");
    assert!(plugin_insertions_only(t, "a\nx\nb\n// mobiler:plugins — insert above\nc", &ok).is_err(), "an extra line away from an anchor");
    assert!(plugin_insertions_only(t, "a\nB\n// mobiler:plugins — insert above\nc", &ok).is_err(), "a changed template line");
    assert!(plugin_insertions_only(t, "a\nb\n// mobiler:plugins — insert above", &ok).is_err(), "a missing template line");
    // A line merely containing an allowed word (here `false`, from `exported="false"`) is not an insertion.
    let manifest = vec!["<activity android:name=\".X\" android:exported=\"false\" />".to_string()];
    assert!(plugin_insertions_only(t, "a\nb\n    case \"x\": return PluginResponse(ok: false, output: \"\")\n// mobiler:plugins — insert above\nc", &manifest).is_err(), "a line sharing a word with a manifest value");
    // Only a comment anchor takes insertions, not a line that merely mentions `mobiler:`.
    let printed = "a\n    print(\"mobiler: font family\")\nc";
    assert!(plugin_insertions_only(printed, "a\n    \"x\" to XPlugin(),\n    print(\"mobiler: font family\")\nc", &ok).is_err(), "a block above a non-anchor line");
}

/// The exact lines `mobiler plugin add` inserts for a plugin manifest, as plugin.rs renders them: the Android
/// and iOS `register` lines, the iOS stream registration and app-launch hooks, `implementation("…")` per gradle
/// dependency, both forms of a gradle plugin, manifest entries, `<uses-permission>` lines (with a ceiling when
/// the entry carries one), and `KEY: value` per `[ios.info_plist]` entry. SwiftPM packages are not listed: no
/// CLI-managed demo uses one, so a line for one fails the check until it is added here.
fn manifest_insertions(toml_text: &str) -> Vec<String> {
    let v: toml::Value = toml::from_str(toml_text).expect("a bundled plugin manifest parses");
    let strings = |table: &str, key: &str| -> Vec<String> {
        match v.get(table).and_then(|t| t.get(key)) {
            Some(toml::Value::String(s)) => vec![s.clone()],
            Some(toml::Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    };
    let yaml = |x: &toml::Value| -> String {
        fn one(x: &toml::Value) -> String {
            match x {
                toml::Value::String(s) => format!("\"{s}\""),
                toml::Value::Array(a) => format!("[{}]", a.iter().map(one).collect::<Vec<_>>().join(", ")),
                other => other.to_string(),
            }
        }
        one(x)
    };
    let mut out = Vec::new();
    out.extend(strings("android", "register"));
    out.extend(strings("android", "manifest_application").iter().map(|m| m.trim().to_string()));
    out.extend(strings("android", "gradle_deps").iter().map(|d| format!("implementation(\"{d}\")")));
    for gp in strings("android", "gradle_plugins") {
        let (id, ver) = gp.split_once(':').unwrap_or((gp.as_str(), ""));
        out.push(format!("id(\"{id}\")"));
        out.push(if ver.is_empty() { format!("id(\"{id}\") apply false") } else { format!("id(\"{id}\") version \"{ver}\" apply false") });
    }
    for p in strings("android", "permissions") {
        out.push(match p.split_once(";maxSdkVersion=") {
            Some((name, max)) => format!("<uses-permission android:name=\"{name}\" android:maxSdkVersion=\"{max}\" />"),
            None => format!("<uses-permission android:name=\"{p}\" />"),
        });
    }
    out.extend(strings("ios", "register"));
    out.extend(strings("ios", "register_stream"));
    out.extend(strings("ios", "app_launch"));
    if let Some(toml::Value::Table(plist)) = v.get("ios").and_then(|i| i.get("info_plist")) {
        out.extend(plist.iter().map(|(k, val)| format!("{k}: {}", yaml(val))));
    }
    out
}

/// Template files an app owns (upgrade's Own and Seed classes): never in the base snapshot, by design.
fn app_owned(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    (rel.starts_with("shared/src/") && rel != "shared/src/bin/codegen.rs")
        || name == "Cargo.toml"
        || rel == "Android/settings.gradle.kts"
        || rel == "Android/app/src/main/res/values/strings.xml"
        || rel == "iOS/Sources/Info.plist"
        || rel.starts_with("iOS/Sources/Assets.xcassets/")
        || ["jar", "webp", "png", "ico"].iter().any(|e| rel.ends_with(&format!(".{e}")))
        || name == "gradlew.bat"
        || rel.ends_with("mobiler_splash.xml")
        || rel == "Android/app/src/main/res/drawable/mobiler_launch.xml"
}

/// ADR-0063 (superseding ADR-0060): every demo but barbershop is CLI-managed and equals the current template.
/// Each demo's `.mobiler/base/` snapshot must equal the template rendered with its names (so a CLI PR that changes
/// a shell without running `mobiler upgrade --apply` in the demo fails), and its shell code must equal the
/// template apart from plugin insertions at anchors, the display name the CLI writes and the blocks its
/// `mobiler.toml` switches (so a hand edit fails). `res/` and `.gitignore` may carry the demo's own edits.
#[test]
fn adr_0063_every_demo_but_barbershop_is_cli_managed() {
    // (dir, package, {{NAME}}, iOS display name the CLI wrote)
    let demos = [
        ("demos/saldo", "rs.mobiler.saldo", "Saldo", Some("Saldo")),
        ("demos/coffee", "dev.mobiler.coffee", "Coffee", None),
        ("demos/todo", "dev.mobiler.todo", "Todo", None),
        ("demos/fullstack-todo/mobile", "dev.mobiler.mobile", "Mobile", Some("FullstackTodo")),
    ];
    for (dir, pkg, name, display) in demos {
        // What the demo's own mobiler.toml switches (ADR-0064's block), read the way the CLI reads it.
        let toml: toml::Value = std::fs::read_to_string(root().join(dir).join("mobiler.toml")).map_or(toml::Value::Table(Default::default()), |t| toml::from_str(&t).expect("mobiler.toml parses"));
        let local_network = toml.get("ios").and_then(|i| i.get("local_network")).and_then(toml::Value::as_bool).unwrap_or(false);
        assert_cli_managed(dir, pkg, name, display, local_network);
    }
    // fullstack-todo talks to its own server over plain HTTP: the option must stay on there.
    assert!(
        std::fs::read_to_string(root().join("demos/fullstack-todo/mobile/mobiler.toml")).unwrap_or_default().contains("local_network = true"),
        "ADR-0063: fullstack-todo's mobiler.toml must ask for local networking"
    );
}

/// `text` with the `# off: ` lines between `begin` and `end` switched on, as `mobiler … sync` writes them.
fn switch_on_block(text: &str, begin: &str, end: &str) -> String {
    let mut inside = false;
    text.lines()
        .map(|l| {
            if l.trim() == begin { inside = true } else if l.trim() == end { inside = false }
            let t = l.trim_start();
            match t.strip_prefix("# off: ") {
                Some(rest) if inside => format!("{}{rest}\n", &l[..l.len() - t.len()]),
                _ => format!("{l}\n"),
            }
        })
        .collect()
}

fn assert_cli_managed(dir: &str, pkg: &str, name: &str, display: Option<&str>, local_network: bool) {
    let pkg_path = pkg.replace('.', "/");
    let app = root().join(dir);
    let version = std::fs::read_to_string(app.join(".mobiler/version")).unwrap_or_else(|_| panic!("ADR-0063: {dir}/.mobiler/version is missing"));
    // The CLI's own version: the first `version = "…"` line of its manifest is the package's.
    let cli = read("mobiler/Cargo.toml").lines().find_map(|l| l.strip_prefix("version = \"")?.split('"').next().map(str::to_string));
    assert_eq!(Some(version.trim().to_string()), cli, "ADR-0063: {dir} is stamped {version}: run mobiler upgrade --apply in {dir}");
    let base = app.join(".mobiler/base");
    let ndk = std::fs::read_to_string(app.join("Android/shared/build.gradle.kts"))
        .unwrap()
        .lines()
        .find_map(|l| l.split("ndkVersion = \"").nth(1)?.split('"').next().map(str::to_string))
        .unwrap_or_else(|| panic!("ADR-0063: no ndkVersion in {dir}'s Android/shared/build.gradle.kts"));
    let render = |raw: &str| {
        raw.replace("{{PACKAGE_SHARED_TYPES}}", &format!("{pkg}.shared.types"))
            .replace("{{PACKAGE_SHARED}}", &format!("{pkg}.shared"))
            .replace("{{PACKAGE_PATH}}", &pkg_path)
            .replace("{{PACKAGE}}", pkg)
            .replace("{{NDK_VERSION}}", &ndk)
            .replace("{{NAME}}", name)
    };
    let walk = |top: PathBuf| {
        let mut files = Vec::new();
        let mut dirs = vec![top];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { dirs.push(path) } else { files.push(path) }
            }
        }
        files
    };
    let files = walk(base.clone());

    // The bundled plugins Saldo uses (their Android source is in its package), and what they may insert.
    let pkg_dir = app.join(format!("Android/app/src/main/java/{pkg_path}"));
    let mut allowed = Vec::new();
    let mut plugin_files = Vec::new();
    for dir in std::fs::read_dir(root().join("mobiler/plugins")).unwrap().map(|e| e.unwrap().path()) {
        let toml = std::fs::read_to_string(dir.join("mobiler-plugin.toml")).unwrap();
        let mut section = "";
        let mut sources: Vec<(String, String)> = Vec::new();
        for line in toml.lines().map(str::trim) {
            if line.starts_with('[') { section = if line.starts_with("[android") { "android" } else if line.starts_with("[ios") { "ios" } else { "" }; }
            if let Some(list) = line.strip_prefix("sources = [") {
                sources.extend(list.split('"').skip(1).step_by(2).map(|src| (section.to_string(), src.to_string())));
            }
        }
        let used = sources.iter().any(|(sec, src)| sec == "android" && pkg_dir.join(src.rsplit('/').next().unwrap()).exists());
        if !used {
            continue;
        }
        allowed.extend(manifest_insertions(&toml));
        for (sec, src) in sources {
            let name = src.rsplit('/').next().unwrap().to_string();
            let ours = if sec == "android" { pkg_dir.join(&name) } else { app.join("iOS/Sources").join(&name) };
            let bundled = render(&std::fs::read_to_string(dir.join(&src)).unwrap());
            let mine = std::fs::read_to_string(&ours).unwrap_or_else(|_| panic!("ADR-0063: missing {}", ours.display()));
            assert_eq!(mine, bundled, "ADR-0063: {} differs from the bundled plugin's {src}: a plugin is never edited in the app", ours.display());
            plugin_files.push(ours);
        }
    }
    // A plugin no bundled package provides is a hand-written shell edit.
    let template_names: Vec<String> = walk(root().join("mobiler/templates")).iter().filter_map(|p| p.file_name()?.to_str().map(str::to_string)).collect();
    for path in walk(pkg_dir.clone()).into_iter().chain(walk(app.join("iOS/Sources"))) {
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        if (name.ends_with("Plugin.kt") || name.ends_with("Plugin.swift") || name.ends_with("Activity.kt"))
            && !plugin_files.contains(&path) && !template_names.contains(&name)
        {
            panic!("ADR-0063: {} is not from the template or a bundled plugin", path.display());
        }
    }

    // Every template file is in the snapshot (a forgotten upgrade that only adds a file), unless the app owns it.
    for tpl in walk(root().join("mobiler/templates")) {
        let rel = tpl.strip_prefix(root().join("mobiler/templates")).unwrap().to_string_lossy().replace('\\', "/");
        let rel = rel.replace("__PACKAGE_PATH__", &pkg_path);
        let rel = rel.strip_suffix(".tmpl").unwrap_or(&rel).to_string();
        if !app_owned(&rel) {
            assert!(base.join(&rel).exists(), "ADR-0063: the template has {rel}, {dir}'s base doesn't: run mobiler upgrade --apply in {dir}");
        }
    }
    // App-owned, but Saldo keeps them as generated: compared whole.
    for rel in ["iOS/Sources/Info.plist", "Android/settings.gradle.kts"] {
        let tpl = root().join("mobiler/templates").join(rel);
        let raw = std::fs::read_to_string(&tpl).or_else(|_| std::fs::read_to_string(format!("{}.tmpl", tpl.display()))).unwrap();
        assert_eq!(std::fs::read_to_string(app.join(rel)).unwrap(), render(&raw), "ADR-0063: {dir}/{rel} is edited by hand");
    }
    let mut code = 0;
    for path in &files {
        let rel = path.strip_prefix(&base).unwrap().to_string_lossy().replace('\\', "/");
        let tpl = root().join("mobiler/templates").join(rel.replace(&pkg_path, "__PACKAGE_PATH__"));
        let raw = std::fs::read_to_string(&tpl)
            .or_else(|_| std::fs::read_to_string(format!("{}.tmpl", tpl.display())))
            .unwrap_or_else(|_| panic!("ADR-0063: {dir}'s base has {rel}, the template doesn't: run mobiler upgrade --apply in {dir}"));
        let rendered = render(&raw);
        // `[ios] local_network = true` (ADR-0064) switches the template's block on; the base keeps it as shipped.
        let expected = if rel == "iOS/project.yml" && local_network { switch_on_block(&rendered, "# mobiler:local-network-begin", "# mobiler:local-network-end") } else { rendered.clone() };
        assert_eq!(std::fs::read_to_string(path).unwrap(), rendered, "ADR-0063: {dir}'s base is behind the template ({rel}): run mobiler upgrade --apply in {dir}");
        if rel.contains("/res/") || rel.ends_with(".gitignore") {
            continue;
        }
        let ours = std::fs::read_to_string(app.join(&rel)).unwrap_or_else(|_| panic!("ADR-0063: {dir}/{rel} is missing"));
        // `mobiler new --display-name` / `mobiler display-name` write the iOS display name: a CLI line, not a hand
        // edit — exactly Saldo's, once.
        let ours = match (rel.as_str(), display) {
            ("iOS/project.yml", Some(shown)) => {
                let line = format!("        CFBundleDisplayName: \"{shown}\"\n");
                assert_eq!(ours.matches(&line).count(), 1, "ADR-0063: {dir}'s project.yml must name it exactly once: {line}");
                ours.replacen(&line, "", 1)
            }
            _ => ours,
        };
        if let Err(at) = plugin_insertions_only(&expected, &ours, &allowed) {
            panic!("ADR-0063: {dir}/{rel} is edited by hand ({at}); change the template, then upgrade");
        }
        code += 1;
    }
    assert!(code >= 15, "ADR-0063: expected {dir}'s shell code in its base, checked {code} files");
}

/// `src` from `from` up to (not including) the first `to` after it; panics naming `what` when either is missing.
fn between<'a>(src: &'a str, from: &str, to: &str, what: &str) -> &'a str {
    let start = src.find(from).unwrap_or_else(|| panic!("{what}: no `{from}`"));
    let len = src[start..].find(to).unwrap_or_else(|| panic!("{what}: no `{to}` after `{from}`"));
    &src[start..start + len]
}

/// ADR-0065: `sqlite` calls run one at a time, in the order the core issued them, on one connection. iOS: the shell
/// chains each call to an in-order plugin behind the previous one, on the main actor, as the effect arrives (a bare
/// `Task` per call would let them reach the plugin in any order), and the plugin runs on one serial queue that owns
/// the one connection. Android: the shell starts a request on the main dispatcher straight into the plugin, and the
/// plugin hops onto one single-thread lane that owns the one connection. Barbershop (ADR-0015) carries the same shell
/// code and plugin; the CLI-managed demos' plugin copies are held equal to the bundled one by ADR-0063's test.
#[test]
fn adr_0065_sqlite_calls_run_in_issue_order() {
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut starts = Vec::new();
    for path in ["mobiler/templates/iOS/Sources/Core.swift", "demos/barbershop/iOS/Sources/Core.swift"] {
        let code = live_code(&read(path));
        let set = between(&code, "static let inOrder: Set<String> = [", "]", path);
        assert!(set.contains("\"sqlite\""), "ADR-0065: {path}'s Plugins.inOrder doesn't name sqlite");
        let notify = between(&code, "case .pluginNotify(let notify):", "case .plugin(let call):", path);
        let request = between(&code, "case .plugin(let call):", "case .pluginStream(let call):", path);
        for (what, block, needle) in [("notify", notify, "start(notify.plugin)"), ("request", request, "start(call.plugin)")] {
            assert!(block.contains(needle), "ADR-0065: {path} starts a plugin {what} without `start`");
            let own_task = ["Task {", "Task{", "Task(", "Task.detached"].iter().any(|t| block.contains(t));
            assert!(!own_task, "ADR-0065: {path} starts a plugin {what} in its own Task, out of order");
        }
        let start = squash(between(&code, "private func start(", "\n    }\n", path));
        let chain = "let previous = laneTails[plugin] laneTails[plugin] = Task { await previous?.value await call() }";
        assert!(start.contains(chain), "ADR-0065: {path}'s `start` doesn't chain an in-order call behind the previous one");
        assert!(start.contains("guard Plugins.inOrder.contains(plugin) else"), "ADR-0065: {path}'s `start` doesn't check Plugins.inOrder");
        starts.push(start);
    }
    assert_eq!(starts[0], starts[1], "ADR-0065: the template's `start` differs from barbershop's");

    for path in ["mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt", "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt"] {
        let code = live_code(&read(path));
        let request = squash(between(&code, "is Effect.Plugin -> {", "is Effect.PluginStream -> {", path));
        assert!(
            request.contains("viewModelScope.launch { val resp = dispatch(call.plugin, call.op, call.input)"),
            "ADR-0065: {path} must start a request on the main dispatcher straight into the plugin, in issue order"
        );
        // A reply or stream event processed nested inside the batch loop would start its calls before the batch's later ones.
        assert!(
            request.contains("withContext(Dispatchers.Main) { process(core.resolve(id, resp.bincodeSerialize())) }"),
            "ADR-0065: {path} must process a reply in its own main-looper turn"
        );
        let stream = squash(between(&code, "is Effect.PluginStream -> {", "private suspend fun dispatch(", path));
        assert!(
            stream.contains("collect { resp -> withContext(Dispatchers.Main) { process(core.resolve(id, resp.bincodeSerialize())) }"),
            "ADR-0065: {path} must process a stream event in its own main-looper turn"
        );
        let notify = squash(between(&code, "is Effect.PluginNotify -> {", "is Effect.Plugin -> {", path));
        // A notify awaited inline would suspend process() and let a later batch's calls reach the lane first.
        assert!(
            notify.contains("else viewModelScope.launch { dispatch(n.plugin, n.op, n.input) }"),
            "ADR-0065: {path} must launch a notify like a request, never await it in process()"
        );
    }

    let swift = live_code(&read("mobiler/plugins/sqlite/ios/SqlitePlugin.swift"));
    assert!(swift.contains("private static let lane = DispatchQueue(label: \"mobiler.sqlite\")\n"), "ADR-0065: the iOS sqlite lane must be one serial queue");
    assert!(squash(&swift).contains("lane.async { cont.resume(returning: run(op: op, input: input)) }"), "ADR-0065: an iOS sqlite call must run on the lane");
    assert_eq!(swift.matches("sqlite3_open(").count(), 1, "ADR-0065: the iOS sqlite plugin opens one connection, in one place");
    assert!(swift.contains("if let db = connection { return (db, \"\") }") && swift.contains("connection = db"), "ADR-0065: the iOS sqlite connection must be kept");
    assert!(swift.contains("while rc == SQLITE_ROW { rc = sqlite3_step(stmt) }"), "ADR-0065: an iOS exec must run its statement to the end");
    // ok:true means committed: a transaction left open at the end of a call is rolled back and the call fails.
    let swift_run = squash(between(&swift, "private static func run(", "\n    }\n", "SqlitePlugin.swift"));
    assert!(
        swift_run.contains("if sqlite3_get_autocommit(db) == 0 { sqlite3_exec(db, \"ROLLBACK\", nil, nil, nil) return PluginResponse(ok: false"),
        "ADR-0065: an iOS call must not leave a transaction open"
    );

    let kotlin = live_code(&read("mobiler/plugins/sqlite/android/SqlitePlugin.kt"));
    assert!(kotlin.contains("val lane = java.util.concurrent.Executors.newSingleThreadExecutor().asCoroutineDispatcher()"), "ADR-0065: the Android sqlite lane must be one thread");
    assert!(
        kotlin.contains("override suspend fun handle(op: String, input: String): PluginResponse = withContext(NonCancellable + lane) { run(op, input) }"),
        "ADR-0065: an Android sqlite call must run on the lane, and a queued one must not be dropped"
    );
    let kotlin_run = squash(between(&kotlin, "private fun run(", "\n    }\n", "SqlitePlugin.kt"));
    assert!(
        kotlin_run.contains("if (db != null && db.isOpen && db.inTransaction()) { try { while (db.inTransaction()) db.endTransaction() }")
            && kotlin_run.contains("return PluginResponse(false,"),
        "ADR-0065: an Android call must not leave a transaction open"
    );
    // Transaction-control statements are refused before they run (Android's session can't see SAVEPOINT).
    let control = "\"BEGIN\", \"COMMIT\", \"END\", \"ROLLBACK\", \"SAVEPOINT\", \"RELEASE\"";
    assert!(kotlin.contains(&format!("val transactionControl = setOf({control})")), "ADR-0065: the Android plugin must list the transaction-control keywords");
    assert!(
        kotlin.contains("firstKeyword(sql) in transactionControl -> PluginResponse(false,"),
        "ADR-0065: the Android plugin must refuse a transaction-control statement"
    );
    assert!(swift.contains(&format!("transactionControl: Set<String> = [{control}]")), "ADR-0065: the iOS plugin must list the transaction-control keywords");
    assert!(
        squash(&swift).contains("if transactionControl.contains(firstKeyword(sql)) { return PluginResponse(ok: false,"),
        "ADR-0065: the iOS plugin must refuse a transaction-control statement"
    );
    assert_eq!(kotlin.matches("openOrCreateDatabase(").count(), 1, "ADR-0065: the Android sqlite plugin opens one connection, in one place");
    assert!(kotlin.contains("connection = db"), "ADR-0065: the Android sqlite connection must be kept");
    assert!(!kotlin.contains("Dispatchers."), "ADR-0065: the Android sqlite plugin must not run on a shared pool");

    // Barbershop's copies are the bundled plugin, rendered with its package.
    let render = |raw: String| raw.replace("{{PACKAGE_SHARED_TYPES}}", "dev.mobiler.barbershop.shared.types").replace("{{PACKAGE}}", "dev.mobiler.barbershop");
    for (bundled, copy) in [
        ("mobiler/plugins/sqlite/ios/SqlitePlugin.swift", "demos/barbershop/iOS/Sources/SqlitePlugin.swift"),
        ("mobiler/plugins/sqlite/android/SqlitePlugin.kt", "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/SqlitePlugin.kt"),
    ] {
        assert_eq!(read(copy), render(read(bundled)), "ADR-0065: {copy} differs from the bundled {bundled}");
    }
}

