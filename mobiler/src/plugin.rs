//! `mobiler plugin add <source>` — install a plugin into a scaffolded app by copying its
//! native handler files and patching the per-shell registration points. A plugin is a
//! self-describing package directory (`mobiler-plugin.toml` + native sources) — either a local
//! path or one of the FREE samples bundled in the CLI. Paid plugins ship as local licensed
//! packages, never bundled here. Android + iOS only; web degrades gracefully on its own.

use crate::templating::{Subs, substitute};
use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// FREE first-party sample plugins, embedded like the project templates.
static BUNDLED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/plugins");

#[derive(clap::Subcommand)]
pub enum PluginCmd {
    /// Install a plugin (SOURCE = a package directory or a bundled sample name).
    Add { source: String },
    /// List the bundled sample plugins.
    List,
}

pub fn run(cmd: PluginCmd) -> Result<()> {
    match cmd {
        PluginCmd::Add { source } => add(&source),
        PluginCmd::List => list(),
    }
}

// ---------------- manifest ----------------

#[derive(Deserialize)]
struct Manifest {
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    android: Option<PlatformSpec>,
    #[serde(default)]
    ios: Option<IosSpec>,
}

#[derive(Deserialize)]
struct PlatformSpec {
    #[serde(default)]
    sources: Vec<String>,
    register: String,
    #[serde(default)]
    permissions: Vec<String>,
    /// Gradle dependency coordinates (e.g. "com.google.android.gms:play-services-code-scanner:16.1.0").
    /// Each becomes an `implementation("…")` line in the app's build.gradle.kts.
    #[serde(default)]
    gradle_deps: Vec<String>,
    /// XML snippets inserted inside `<application>` in AndroidManifest.xml — e.g. a
    /// `<receiver android:name=".NotificationReceiver" android:exported="false"/>` a plugin needs
    /// to fire while the app is closed. Each should carry a unique `android:name` (used for the
    /// idempotency check).
    #[serde(default)]
    manifest_application: Vec<String>,
}

#[derive(Deserialize)]
struct IosSpec {
    #[serde(default)]
    sources: Vec<String>,
    register: String,
    #[serde(default)]
    info_plist: BTreeMap<String, String>,
    #[serde(default)]
    entitlements: BTreeMap<String, toml::Value>,
}

// ---------------- source resolution ----------------

enum Source {
    Local(PathBuf),
    Bundled(&'static Dir<'static>),
}

impl Source {
    fn read_text(&self, rel: &str) -> Result<String> {
        match self {
            Source::Local(dir) => fs::read_to_string(dir.join(rel))
                .with_context(|| format!("reading {}", dir.join(rel).display())),
            Source::Bundled(d) => {
                // `Dir::get_file` resolves relative to the embed ROOT, so a file inside a
                // sub-dir needs its full path (e.g. `battery/mobiler-plugin.toml`).
                let full = d.path().join(rel);
                d.get_file(&full)
                    .and_then(|f| f.contents_utf8())
                    .map(str::to_string)
                    .ok_or_else(|| anyhow::anyhow!("bundled plugin file `{rel}` missing or not UTF-8"))
            }
        }
    }
}

fn resolve_source(source: &str) -> Result<Source> {
    let p = Path::new(source);
    if p.is_dir() {
        return Ok(Source::Local(p.to_path_buf()));
    }
    if let Some(dir) = BUNDLED.get_dir(source) {
        return Ok(Source::Bundled(dir));
    }
    bail!(
        "`{source}` is neither a plugin directory nor a bundled sample. Bundled: {}",
        bundled_names().join(", ")
    )
}

fn bundled_names() -> Vec<String> {
    BUNDLED
        .dirs()
        .filter_map(|d| d.path().file_name().map(|n| n.to_string_lossy().to_string()))
        .collect()
}

// ---------------- list ----------------

fn list() -> Result<()> {
    let names = bundled_names();
    if names.is_empty() {
        println!("No bundled sample plugins.");
    } else {
        println!("Bundled sample plugins (free):");
        for name in names {
            let summary = resolve_source(&name)
                .and_then(|s| s.read_text("mobiler-plugin.toml"))
                .ok()
                .and_then(|t| toml::from_str::<Manifest>(&t).ok())
                .map(|m| m.summary)
                .unwrap_or_default();
            println!("  {name:<12} {summary}");
        }
    }
    println!("\nInstall:  mobiler plugin add <name>   |   mobiler plugin add ./path/to/package");
    Ok(())
}

// ---------------- add ----------------

fn add(source: &str) -> Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    add_at(&root, source)
}

fn add_at(root: &Path, source: &str) -> Result<()> {
    if !root.join("Android").is_dir() || !root.join("iOS").is_dir() {
        bail!("run `mobiler plugin add` from a Mobiler app root (the dir with Android/ and iOS/)");
    }
    let subs = Subs::from_app_root(root)?;
    let src = resolve_source(source)?;
    let manifest: Manifest = toml::from_str(&src.read_text("mobiler-plugin.toml")?)
        .context("parsing mobiler-plugin.toml")?;

    println!("Installing plugin `{}`{}", manifest.name, fmt_summary(&manifest.summary));
    let mut notes: Vec<String> = Vec::new();

    if let Some(a) = &manifest.android {
        let dst_dir = root.join("Android/app/src/main/java").join(&subs.package_path);
        for rel in &a.sources {
            copy_source(&src, rel, &dst_dir, &subs, root)?;
        }
        let core_kt = dst_dir.join("Core.kt");
        report(insert_before(&core_kt, "// mobiler:plugins", &format!("{},", a.register), &a.register)?, "Android registration");
        let manifest_xml = root.join("Android/app/src/main/AndroidManifest.xml");
        for perm in &a.permissions {
            let line = format!("<uses-permission android:name=\"{perm}\" />");
            report(insert_before(&manifest_xml, "mobiler:permissions", &line, perm)?, "Android permission");
        }
        let gradle = root.join("Android/app/build.gradle.kts");
        for dep in &a.gradle_deps {
            let line = format!("implementation(\"{dep}\")");
            report(insert_before(&gradle, "mobiler:gradle-deps", &line, dep)?, "Android Gradle dependency");
        }
        for xml in &a.manifest_application {
            // Idempotency key: the snippet's android:name (unique per receiver/service/provider),
            // falling back to the trimmed snippet if it has none.
            let needle = manifest_name(xml).unwrap_or_else(|| xml.trim().to_string());
            report(insert_before(&manifest_xml, "mobiler:manifest-application", xml.trim(), &needle)?, "Android manifest entry");
        }
    }

    if let Some(i) = &manifest.ios {
        let dst_dir = root.join("iOS/Sources");
        for rel in &i.sources {
            copy_source(&src, rel, &dst_dir, &subs, root)?;
        }
        let core_swift = root.join("iOS/Sources/Core.swift");
        report(insert_before(&core_swift, "// mobiler:plugins", &i.register, &i.register)?, "iOS registration");

        let project_yml = root.join("iOS/project.yml");
        for (key, val) in &i.info_plist {
            let line = format!("{key}: \"{val}\"");
            report(insert_before(&project_yml, "# mobiler:info-plist", &line, key)?, "iOS Info.plist key");
        }
        if !i.entitlements.is_empty() {
            install_entitlements(&project_yml, &i.entitlements, &mut notes)?;
            notes.push(
                "iOS: enable the matching capability on your App ID in the Apple Developer \
                 portal (the one step that can't be automated)."
                    .to_string(),
            );
        }
    }

    println!("\n✓ Plugin `{}` installed.", manifest.name);
    for n in notes {
        println!("  • {n}");
    }
    Ok(())
}

fn fmt_summary(s: &str) -> String {
    if s.is_empty() { String::new() } else { format!(" — {s}") }
}

fn report(res: Insert, what: &str) {
    match res {
        Insert::Inserted => println!("  + {what}"),
        Insert::AlreadyPresent => println!("  · {what} already present (skipped)"),
        Insert::MarkerMissing(m) => println!("  ! {what}: anchor `{m}` not found — add it manually"),
    }
}

/// Extract the value of `android:name="…"` from a manifest XML snippet (the idempotency key
/// for a `<receiver>` / `<service>` / `<provider>` entry).
fn manifest_name(xml: &str) -> Option<String> {
    let key = "android:name=\"";
    let start = xml.find(key)? + key.len();
    let end = xml[start..].find('"')? + start;
    Some(xml[start..end].to_string())
}

/// Copy one plugin source file into `dst_dir`, substituting `{{PACKAGE}}` etc.
fn copy_source(src: &Source, rel: &str, dst_dir: &Path, subs: &Subs, root: &Path) -> Result<()> {
    let raw = src.read_text(rel)?;
    let name = Path::new(rel)
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("plugin source `{rel}` has no file name"))?;
    fs::create_dir_all(dst_dir).with_context(|| format!("creating {}", dst_dir.display()))?;
    let dst = dst_dir.join(name);
    fs::write(&dst, substitute(&raw, subs)).with_context(|| format!("writing {}", dst.display()))?;
    println!("  + {}", dst.strip_prefix(root).unwrap_or(&dst).display());
    Ok(())
}

// ---------------- marker-based insertion ----------------

enum Insert {
    Inserted,
    AlreadyPresent,
    MarkerMissing(String),
}

/// Insert `payload` (one logical line) immediately before the line containing `marker`,
/// matching the marker's indentation. Idempotent: skip if `needle` is already in the file.
fn insert_before(path: &Path, marker: &str, payload: &str, needle: &str) -> Result<Insert> {
    let content = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if content.contains(needle) {
        return Ok(Insert::AlreadyPresent);
    }
    let Some(marker_line) = content.lines().find(|l| l.contains(marker)) else {
        return Ok(Insert::MarkerMissing(marker.to_string()));
    };
    let indent: String = marker_line.chars().take_while(|c| c.is_whitespace()).collect();
    let anchor = format!("{marker_line}\n");
    let updated = content.replacen(&anchor, &format!("{indent}{payload}\n{anchor}"), 1);
    fs::write(path, updated).with_context(|| format!("writing {}", path.display()))?;
    Ok(Insert::Inserted)
}

/// Add an xcodegen target-level `entitlements:` block at the `# mobiler:target-extra` anchor.
/// v1 handles the create case; if a block already exists, leaves a note to merge by hand.
fn install_entitlements(
    project_yml: &Path,
    entitlements: &BTreeMap<String, toml::Value>,
    notes: &mut Vec<String>,
) -> Result<()> {
    let content = fs::read_to_string(project_yml)?;
    if content.lines().any(|l| l.trim_start().starts_with("entitlements:")) {
        notes.push(format!(
            "iOS: an `entitlements:` block already exists in project.yml — add these keys by hand: {}",
            entitlements.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
        return Ok(());
    }
    let Some(marker_line) = content.lines().find(|l| l.contains("# mobiler:target-extra")) else {
        notes.push("iOS: anchor `# mobiler:target-extra` not found — add the entitlements block manually.".into());
        return Ok(());
    };
    let indent: String = marker_line.chars().take_while(|c| c.is_whitespace()).collect();
    let mut block =
        format!("{indent}entitlements:\n{indent}  path: Sources/App.entitlements\n{indent}  properties:\n");
    for (key, val) in entitlements {
        block.push_str(&format!("{indent}    {key}: {}\n", yaml_scalar(val)));
    }
    let anchor = format!("{marker_line}\n");
    let updated = content.replacen(&anchor, &format!("{block}{anchor}"), 1);
    fs::write(project_yml, updated)?;
    println!("  + iOS entitlements block");
    Ok(())
}

/// Render a TOML scalar/array as an inline YAML value (strings, bools, ints, arrays of strings).
fn yaml_scalar(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => format!("\"{s}\""),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Array(a) => {
            let items: Vec<String> = a.iter().map(yaml_scalar).collect();
            format!("[{}]", items.join(", "))
        }
        other => format!("\"{other}\""),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A throwaway temp dir with a minimal Mobiler app skeleton carrying the anchor markers.
    fn skeleton() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("mob_plugin_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let pkg = root.join("Android/app/src/main/java/dev/mobiler/demo");
        fs::create_dir_all(&pkg).unwrap();
        fs::create_dir_all(root.join("iOS/Sources")).unwrap();
        fs::write(pkg.join("MainActivity.kt"), "package dev.mobiler.demo\nclass MainActivity\n").unwrap();
        fs::write(
            pkg.join("Core.kt"),
            "package dev.mobiler.demo\nval plugins = mapOf(\n    \"http\" to HttpPlugin(),\n    // mobiler:plugins\n)\n",
        )
        .unwrap();
        fs::write(
            root.join("Android/app/src/main/AndroidManifest.xml"),
            "<manifest>\n    <!-- mobiler:permissions -->\n    <application>\n        <!-- mobiler:manifest-application -->\n    </application>\n</manifest>\n",
        )
        .unwrap();
        fs::write(root.join("Android/settings.gradle.kts"), "rootProject.name = \"Demo\"\n").unwrap();
        fs::write(
            root.join("Android/app/build.gradle.kts"),
            "dependencies {\n    implementation(project(\":shared\"))\n    // mobiler:gradle-deps\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("iOS/Sources/Core.swift"),
            "switch plugin {\n        case \"http\": return x\n        // mobiler:plugins\n        default: return y\n        }\n",
        )
        .unwrap();
        fs::write(
            root.join("iOS/project.yml"),
            "targets:\n  Demo:\n    info:\n      properties:\n        PRODUCT_BUNDLE_IDENTIFIER: dev.mobiler.demo\n        # mobiler:info-plist\n    settings:\n      base:\n        FOO: bar\n    # mobiler:target-extra\n",
        )
        .unwrap();
        root
    }

    fn read(root: &Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap()
    }

    #[test]
    fn add_bundled_battery_copies_and_registers() {
        let root = skeleton();
        add_at(&root, "battery").unwrap();

        // Native source copied with {{PACKAGE}} substituted to the app's package.
        let kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/BatteryPlugin.kt");
        assert!(kt.contains("package dev.mobiler.demo"), "package substituted");
        // Registered in both shells, before the marker.
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"battery\" to BatteryPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"battery\": return await BatteryPlugin.handle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_connectivity_copies_registers_and_adds_permission() {
        let root = skeleton();
        add_at(&root, "connectivity").unwrap();

        let kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/ConnectivityPlugin.kt");
        assert!(kt.contains("package dev.mobiler.demo"), "package substituted");
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"connectivity\" to ConnectivityPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"connectivity\": return await ConnectivityPlugin.handle"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android.permission.ACCESS_NETWORK_STATE"), "permission injected");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_is_idempotent() {
        let root = skeleton();
        add_at(&root, "battery").unwrap();
        add_at(&root, "battery").unwrap(); // second run must not duplicate
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert_eq!(core_kt.matches("\"battery\" to").count(), 1, "no duplicate registration");
        let _ = fs::remove_dir_all(&root);
    }

    /// Guards against template/installer drift: the REAL scaffold templates must carry every
    /// anchor `insert_before` targets, or `plugin add` silently no-ops on a real app (which a
    /// skeleton-based test can't catch — this is exactly the bug that slipped through once).
    #[test]
    fn templates_carry_every_anchor() {
        let t = |rel: &str| {
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/templates").to_string() + "/" + rel)
                .unwrap_or_else(|_| panic!("missing template {rel}"))
        };
        assert!(
            t("Android/app/src/main/java/__PACKAGE_PATH__/Core.kt").contains("// mobiler:plugins"),
            "Core.kt needs the // mobiler:plugins anchor"
        );
        assert!(
            t("iOS/Sources/Core.swift").contains("// mobiler:plugins"),
            "Core.swift needs the // mobiler:plugins anchor"
        );
        let manifest = t("Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("mobiler:permissions"), "AndroidManifest.xml needs the mobiler:permissions anchor");
        assert!(manifest.contains("mobiler:manifest-application"), "AndroidManifest.xml needs the mobiler:manifest-application anchor");
        let yml = t("iOS/project.yml");
        assert!(yml.contains("# mobiler:info-plist"), "project.yml needs the info-plist anchor");
        assert!(yml.contains("# mobiler:target-extra"), "project.yml needs the target-extra anchor");
        assert!(
            t("Android/app/build.gradle.kts").contains("mobiler:gradle-deps"),
            "build.gradle.kts needs the mobiler:gradle-deps anchor"
        );
    }

    #[test]
    fn add_local_package_patches_permission_and_entitlements() {
        let root = skeleton();
        // A tiny local plugin package with a permission, an Info.plist key, and an entitlement.
        let pkg = std::env::temp_dir().join(format!("mob_pkg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&pkg);
        fs::create_dir_all(pkg.join("android")).unwrap();
        fs::create_dir_all(pkg.join("ios")).unwrap();
        fs::write(pkg.join("android/FooPlugin.kt"), "package {{PACKAGE}}\nclass FooPlugin\n").unwrap();
        fs::write(pkg.join("ios/FooPlugin.swift"), "enum FooPlugin {}\n").unwrap();
        fs::write(
            pkg.join("mobiler-plugin.toml"),
            r#"name = "foo"
[android]
sources = ["android/FooPlugin.kt"]
register = '"foo" to FooPlugin(application)'
permissions = ["android.permission.NFC"]
gradle_deps = ["com.example:foo:1.2.3"]
manifest_application = ['<receiver android:name=".FooReceiver" android:exported="false"/>']
[ios]
sources = ["ios/FooPlugin.swift"]
register = 'case "foo": return await FooPlugin.handle(op: op, input: input)'
[ios.info_plist]
NFCReaderUsageDescription = "use nfc"
[ios.entitlements]
"com.apple.developer.nfc.readersession.formats" = ["NDEF"]
"#,
        )
        .unwrap();

        add_at(&root, pkg.to_str().unwrap()).unwrap();

        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android.permission.NFC"), "permission added");
    let gradle = read(&root, "Android/app/build.gradle.kts");
    assert!(gradle.contains("implementation(\"com.example:foo:1.2.3\")"), "gradle dep added");
        let project = read(&root, "iOS/project.yml");
        assert!(project.contains("NFCReaderUsageDescription: \"use nfc\""), "info.plist key added");
        assert!(
            project.contains("entitlements:")
                && project.contains("com.apple.developer.nfc.readersession.formats")
                && project.contains("NDEF"),
            "entitlements added"
        );
        let kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/FooPlugin.kt");
        assert!(kt.contains("package dev.mobiler.demo"), "package substituted in local source");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&pkg);
    }
}
