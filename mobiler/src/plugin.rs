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
    /// Free-form post-install notes printed after a successful add — for the one or two steps a
    /// plugin can't automate (e.g. push: drop `google-services.json` into `Android/app/`).
    #[serde(default)]
    notes: Vec<String>,
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
    /// Gradle plugin coordinates "plugin.id:version" (e.g. "com.google.gms.google-services:4.4.2").
    /// Each adds `id("plugin.id") version "version" apply false` to the **project** build.gradle.kts
    /// (`// mobiler:gradle-plugins-classpath`) and `id("plugin.id")` to the **app** build.gradle.kts
    /// (`// mobiler:gradle-plugins`) — for plugins that must be APPLIED (e.g. google-services), which a
    /// plain `gradle_deps` `implementation(…)` line can't express.
    #[serde(default)]
    gradle_plugins: Vec<String>,
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
    /// Optional streaming-dispatch registration (a `Plugins.subscribe` case), inserted at the
    /// `// mobiler:plugins-stream` marker. Set by streaming-capable plugins (e.g. websocket) in
    /// addition to `register` (their request/response case).
    #[serde(default)]
    register_stream: Option<String>,
    #[serde(default)]
    info_plist: BTreeMap<String, String>,
    #[serde(default)]
    entitlements: BTreeMap<String, toml::Value>,
    /// Remote SwiftPM packages, each "name|url|version|product" (pipe-delimited — URLs contain `:`/`/`).
    /// Adds a remote package entry to the iOS project.yml `packages:` block (`# mobiler:spm-packages`)
    /// AND a target `- package: <name> / product: <product>` dependency (`# mobiler:spm-dependencies`)
    /// — the iOS twin of `android.gradle_plugins`, for plugins that pull an SPM dependency (e.g. Firebase).
    #[serde(default)]
    spm_packages: Vec<String>,
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
        let gradle_proj = root.join("Android/build.gradle.kts");
        for gp in &a.gradle_plugins {
            // "plugin.id:version" → applied in the app block + declared (apply false) at project level.
            let (id, ver) = gp.split_once(':').unwrap_or((gp.as_str(), ""));
            let needle = format!("id(\"{id}\")");
            report(insert_before(&gradle, "mobiler:gradle-plugins", &needle, &needle)?, "Android Gradle plugin (app)");
            let proj_line = if ver.is_empty() {
                format!("id(\"{id}\") apply false")
            } else {
                format!("id(\"{id}\") version \"{ver}\" apply false")
            };
            report(insert_before(&gradle_proj, "mobiler:gradle-plugins-classpath", &proj_line, &needle)?, "Android Gradle plugin (project)");
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
        if let Some(rs) = &i.register_stream {
            report(insert_before(&core_swift, "// mobiler:plugins-stream", rs, rs)?, "iOS streaming registration");
        }

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
        for entry in &i.spm_packages {
            install_spm_package(&project_yml, entry)?;
        }
    }

    notes.extend(manifest.notes.iter().cloned());
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

/// Whether `line` contains `marker` at a token boundary — i.e. `marker` is NOT immediately
/// followed by `-`, `_`, or an alphanumeric. This stops a marker from matching a longer marker
/// that has it as a prefix (e.g. `mobiler:plugins` must not match `mobiler:plugins-stream`).
fn line_has_marker(line: &str, marker: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = line[from..].find(marker) {
        let end = from + pos + marker.len();
        let next = line[end..].chars().next();
        if !matches!(next, Some(c) if c == '-' || c == '_' || c.is_alphanumeric()) {
            return true;
        }
        from = end;
    }
    false
}

/// Insert `payload` (one logical line) immediately before the line containing `marker` (matched at
/// a token boundary), with the marker's indentation. Idempotent: skip if `needle` is already present.
fn insert_before(path: &Path, marker: &str, payload: &str, needle: &str) -> Result<Insert> {
    let content = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if content.contains(needle) {
        return Ok(Insert::AlreadyPresent);
    }
    let Some(marker_line) = content.lines().find(|l| line_has_marker(l, marker)) else {
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

/// Add a remote SwiftPM package (`name|url|version|product`) to the iOS project.yml: a package entry
/// in the top-level `packages:` block (at `# mobiler:spm-packages`) + a target product dependency in
/// `dependencies:` (at `# mobiler:spm-dependencies`). Idempotent — skips the package if its `url` is
/// already present, and the dependency if `product: <product>` is. The iOS twin of `gradle_plugins`.
fn install_spm_package(project_yml: &Path, entry: &str) -> Result<()> {
    let parts: Vec<&str> = entry.split('|').collect();
    let [name, url, version, product] = parts[..] else {
        bail!("spm_packages entry must be 'name|url|version|product', got `{entry}`");
    };
    let mut content =
        fs::read_to_string(project_yml).with_context(|| format!("reading {}", project_yml.display()))?;

    // 1) Remote package entry in the `packages:` block.
    if !content.contains(url) {
        match content.lines().find(|l| line_has_marker(l, "# mobiler:spm-packages")).map(str::to_string) {
            Some(marker) => {
                let indent: String = marker.chars().take_while(|c| c.is_whitespace()).collect();
                let block = format!("{indent}{name}:\n{indent}  url: {url}\n{indent}  from: {version}\n");
                let anchor = format!("{marker}\n");
                content = content.replacen(&anchor, &format!("{block}{anchor}"), 1);
                println!("  + iOS SwiftPM package {name}");
            }
            None => println!("  ! iOS SwiftPM package: anchor `# mobiler:spm-packages` not found — add it manually"),
        }
    }

    // 2) Target product dependency in the `dependencies:` block.
    let dep_needle = format!("product: {product}");
    if !content.contains(&dep_needle) {
        match content.lines().find(|l| line_has_marker(l, "# mobiler:spm-dependencies")).map(str::to_string) {
            Some(marker) => {
                let indent: String = marker.chars().take_while(|c| c.is_whitespace()).collect();
                let block = format!("{indent}- package: {name}\n{indent}  product: {product}\n");
                let anchor = format!("{marker}\n");
                content = content.replacen(&anchor, &format!("{block}{anchor}"), 1);
                println!("  + iOS SwiftPM dependency {product}");
            }
            None => println!("  ! iOS SwiftPM dependency: anchor `# mobiler:spm-dependencies` not found — add it manually"),
        }
    }

    fs::write(project_yml, content).with_context(|| format!("writing {}", project_yml.display()))?;
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
            root.join("Android/build.gradle.kts"),
            "plugins {\n    alias(libs.plugins.android.application) apply false\n    // mobiler:gradle-plugins-classpath\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("Android/app/build.gradle.kts"),
            "plugins {\n    alias(libs.plugins.android.application)\n    // mobiler:gradle-plugins\n}\ndependencies {\n    implementation(project(\":shared\"))\n    // mobiler:gradle-deps\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("iOS/Sources/Core.swift"),
            "func subscribe() {\n        switch plugin {\n        // mobiler:plugins-stream\n        default: break\n        }\n    }\n    func handle() {\n        switch plugin {\n        case \"http\": return x\n        // mobiler:plugins\n        default: return y\n        }\n    }\n",
        )
        .unwrap();
        fs::write(
            root.join("iOS/project.yml"),
            "packages:\n  SharedTypes:\n    path: generated/SharedTypes\n  # mobiler:spm-packages\ntargets:\n  Demo:\n    dependencies:\n      - package: SharedTypes\n      # mobiler:spm-dependencies\n    info:\n      properties:\n        PRODUCT_BUNDLE_IDENTIFIER: dev.mobiler.demo\n        # mobiler:info-plist\n    settings:\n      base:\n        FOO: bar\n    # mobiler:target-extra\n",
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
    fn line_has_marker_respects_token_boundaries() {
        // A marker must not match a longer marker that has it as a prefix.
        assert!(line_has_marker("    // mobiler:plugins — inserts here", "// mobiler:plugins"));
        assert!(!line_has_marker("    // mobiler:plugins-stream — inserts here", "// mobiler:plugins"));
        assert!(line_has_marker("    // mobiler:plugins-stream — inserts here", "// mobiler:plugins-stream"));
    }

    #[test]
    fn add_bundled_websocket_registers_handle_and_stream_cases() {
        let root = skeleton();
        add_at(&root, "websocket").unwrap();

        let core_swift = read(&root, "iOS/Sources/Core.swift");
        // The request/response case lands at // mobiler:plugins …
        let handle_at = core_swift.find("case \"websocket\": return await WebSocketPlugin.handle").expect("handle case");
        // … and the streaming case lands at the // mobiler:plugins-stream marker.
        let stream_at = core_swift.find("case \"websocket\": await WebSocketPlugin.subscribe").expect("stream case");
        // Crucially, each lands in its OWN switch — the stream case (in `subscribe`, earlier in the
        // file) before the handle case (in `handle`). Guards the marker prefix-collision bug:
        // `// mobiler:plugins` must NOT match the `// mobiler:plugins-stream` line.
        assert!(stream_at < handle_at, "stream case must be in subscribe(), handle case in handle()");
        assert!(handle_at > core_swift.find("func handle()").unwrap(), "handle case must be inside handle()");
        // Android registers once (its streaming is the polymorphic subscribe() override).
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"websocket\" to WebSocketPlugin(application),"));
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
    fn add_bundled_filepicker_copies_both_sources_and_registers_activity() {
        let root = skeleton();
        add_at(&root, "filepicker").unwrap();

        // Both Android sources copied with the package substituted.
        let plugin = read(&root, "Android/app/src/main/java/dev/mobiler/demo/FilePickerPlugin.kt");
        assert!(plugin.contains("package dev.mobiler.demo"));
        let activity = read(&root, "Android/app/src/main/java/dev/mobiler/demo/FilePickerActivity.kt");
        assert!(activity.contains("class FilePickerActivity"));
        // Registered in both shells + the helper Activity declared in the manifest.
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"filepicker\" to FilePickerPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"filepicker\": return await FilePickerPlugin.handle"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android:name=\".FilePickerActivity\""), "helper activity declared");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_files_copies_sources_registers_and_declares_export_activity() {
        let root = skeleton();
        add_at(&root, "files").unwrap();

        // Both Android sources copied with the package substituted.
        let plugin = read(&root, "Android/app/src/main/java/dev/mobiler/demo/FilesPlugin.kt");
        assert!(plugin.contains("package dev.mobiler.demo"));
        assert!(plugin.contains("class FilesPlugin"));
        let activity = read(&root, "Android/app/src/main/java/dev/mobiler/demo/FileExportActivity.kt");
        assert!(activity.contains("class FileExportActivity"));
        // Registered in both shells + the export helper Activity declared in the manifest.
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"files\" to FilesPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"files\": return await FilesPlugin.handle"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android:name=\".FileExportActivity\""), "export helper activity declared");
        let swift = read(&root, "iOS/Sources/FilesPlugin.swift");
        assert!(swift.contains("enum FilesPlugin"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_geolocation_adds_permissions_and_plist_key() {
        let root = skeleton();
        add_at(&root, "geolocation").unwrap();

        let kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/GeolocationPlugin.kt");
        assert!(kt.contains("package dev.mobiler.demo"));
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"geolocation\" to GeolocationPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"geolocation\": return await GeolocationPlugin.handle"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android.permission.ACCESS_FINE_LOCATION"), "fine perm injected");
        assert!(manifest.contains("android.permission.ACCESS_COARSE_LOCATION"), "coarse perm injected");
        let yml = read(&root, "iOS/project.yml");
        assert!(yml.contains("NSLocationWhenInUseUsageDescription"), "iOS usage-description injected");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_sensors_copies_and_registers() {
        let root = skeleton();
        add_at(&root, "sensors").unwrap();
        let kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/SensorsPlugin.kt");
        assert!(kt.contains("package dev.mobiler.demo"));
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"sensors\" to SensorsPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"sensors\": return await SensorsPlugin.handle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_contacts_copies_both_sources_and_registers_activity() {
        let root = skeleton();
        add_at(&root, "contacts").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/ContactsPlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/ContactsPickerActivity.kt").contains("class ContactsPickerActivity"));
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"contacts\" to ContactsPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"contacts\": return await ContactsPlugin.handle"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android:name=\".ContactsPickerActivity\""));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_calendar_registers_and_adds_plist_keys() {
        let root = skeleton();
        add_at(&root, "calendar").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/CalendarPlugin.kt").contains("package dev.mobiler.demo"));
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"calendar\" to CalendarPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"calendar\": return await CalendarPlugin.handle"));
        let yml = read(&root, "iOS/project.yml");
        assert!(yml.contains("NSCalendarsUsageDescription"), "iOS calendar usage key injected");
        assert!(yml.contains("NSCalendarsWriteOnlyAccessUsageDescription"), "iOS 17+ write-only key injected");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_audio_adds_record_permission_and_mic_plist_key() {
        let root = skeleton();
        add_at(&root, "audio").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/AudioPlugin.kt").contains("package dev.mobiler.demo"));
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"audio\" to AudioPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        assert!(core_swift.contains("case \"audio\": return await AudioPlugin.handle"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android.permission.RECORD_AUDIO"), "RECORD_AUDIO injected");
        let yml = read(&root, "iOS/project.yml");
        assert!(yml.contains("NSMicrophoneUsageDescription"), "mic usage key injected");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_composer_copies_and_registers() {
        let root = skeleton();
        add_at(&root, "composer").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/ComposerPlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"composer\" to ComposerPlugin(application),"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"composer\": return await ComposerPlugin.handle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_tts_copies_and_registers() {
        let root = skeleton();
        add_at(&root, "tts").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/TtsPlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"tts\" to TtsPlugin(application),"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"tts\": return await TtsPlugin.handle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_review_registers_and_adds_gradle_dep() {
        let root = skeleton();
        add_at(&root, "review").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"review\" to ReviewPlugin(application),"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"review\": return await ReviewPlugin.handle"));
        assert!(read(&root, "Android/app/build.gradle.kts").contains("com.google.android.play:review"), "gradle dep injected");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_sharefile_copies_and_registers() {
        let root = skeleton();
        add_at(&root, "sharefile").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/SharefilePlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"sharefile\" to SharefilePlugin(application),"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"sharefile\": return await SharefilePlugin.handle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_video_registers_activity_and_plist_keys() {
        let root = skeleton();
        add_at(&root, "video").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/VideoCaptureActivity.kt").contains("class VideoCaptureActivity"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"video\" to VideoPlugin(application),"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"video\": return await VideoPlugin.handle"));
        assert!(read(&root, "Android/app/src/main/AndroidManifest.xml").contains("android:name=\".VideoCaptureActivity\""));
        assert!(read(&root, "iOS/project.yml").contains("NSCameraUsageDescription"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_speech_registers_both_platforms_and_plist() {
        let root = skeleton();
        add_at(&root, "speech").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/SpeechActivity.kt").contains("class SpeechActivity"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"speech\" to SpeechPlugin(application),"));
        assert!(read(&root, "Android/app/src/main/AndroidManifest.xml").contains("android:name=\".SpeechActivity\""));
        // iOS now shipped: registration + usage-description plist keys.
        assert!(read(&root, "iOS/Sources/SpeechPlugin.swift").contains("SFSpeechRecognizer"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"speech\": return await SpeechPlugin.handle"));
        let yml = read(&root, "iOS/project.yml");
        assert!(yml.contains("NSSpeechRecognitionUsageDescription"));
        assert!(yml.contains("NSMicrophoneUsageDescription"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_sqlite_registers_both_platforms() {
        let root = skeleton();
        add_at(&root, "sqlite").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/SqlitePlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"sqlite\" to SqlitePlugin(application),"));
        // iOS now shipped.
        assert!(read(&root, "iOS/Sources/SqlitePlugin.swift").contains("import SQLite3"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"sqlite\": return await SqlitePlugin.handle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_bluetooth_adds_perms_registration_and_plist() {
        let root = skeleton();
        add_at(&root, "bluetooth").unwrap();
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/BluetoothPlugin.kt").contains("BluetoothGattCallback"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"bluetooth\" to BluetoothPlugin(application),"));
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android.permission.BLUETOOTH_SCAN"));
        assert!(manifest.contains("android.permission.BLUETOOTH_CONNECT"));
        assert!(read(&root, "iOS/Sources/BluetoothPlugin.swift").contains("CBCentralManager"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"bluetooth\": return await BluetoothPlugin.handle"));
        assert!(read(&root, "iOS/project.yml").contains("NSBluetoothAlwaysUsageDescription"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_oauth_copies_sources_registers_and_declares_redirect_activity() {
        let root = skeleton();
        add_at(&root, "oauth").unwrap();
        // Both Android sources copied with the package substituted.
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/OAuthPlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/OAuthActivity.kt").contains("class OAuthActivity"));
        // Registered in both shells.
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt").contains("\"oauth\" to OAuthPlugin(application),"));
        assert!(read(&root, "iOS/Sources/Core.swift").contains("case \"oauth\": return await OAuthPlugin.handle"));
        // Redirect-catcher Activity declared in the manifest, keyed on ${applicationId}.
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android:name=\".OAuthActivity\""), "redirect activity declared");
        assert!(manifest.contains("android:scheme=\"${applicationId}\""), "redirect scheme = applicationId");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_push_registers_service_perm_dep_gradle_plugin_and_entitlement() {
        let root = skeleton();
        add_at(&root, "push").unwrap();

        // Both Android sources copied with the package substituted.
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/PushPlugin.kt").contains("package dev.mobiler.demo"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/PushMessagingService.kt").contains("class PushMessagingService"));
        // Registered in both shells — handle case + the streaming case (cx.subscribe).
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"push\" to PushPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        let handle_at = core_swift.find("case \"push\": return await PushPlugin.handle").expect("handle case");
        let stream_at = core_swift.find("case \"push\": await PushPlugin.subscribe").expect("stream case");
        assert!(stream_at < handle_at, "stream case in subscribe(), handle case in handle()");
        // Permission + FCM service + firebase dep.
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android.permission.POST_NOTIFICATIONS"), "POST_NOTIFICATIONS injected");
        assert!(manifest.contains("android:name=\".PushMessagingService\""), "FCM service declared");
        let gradle = read(&root, "Android/app/build.gradle.kts");
        assert!(gradle.contains("com.google.firebase:firebase-messaging"), "firebase dep injected");
        // The new gradle_plugins field: applied in the app block + declared (apply false) at project level.
        assert!(gradle.contains("id(\"com.google.gms.google-services\")"), "google-services applied in app gradle");
        let gradle_proj = read(&root, "Android/build.gradle.kts");
        assert!(
            gradle_proj.contains("id(\"com.google.gms.google-services\") version \"4.4.2\" apply false"),
            "google-services declared (apply false) in project gradle"
        );
        // iOS aps-environment entitlement (first plugin to exercise the entitlements path).
        let yml = read(&root, "iOS/project.yml");
        assert!(yml.contains("entitlements:") && yml.contains("aps-environment"), "aps-environment entitlement added");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_iap_registers_both_switches_and_adds_billing_dep() {
        let root = skeleton();
        add_at(&root, "iap").unwrap();

        // Source copied with the package substituted.
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/IapPlugin.kt").contains("package dev.mobiler.demo"));
        // Registered in both shells; the streaming case lands in subscribe(), the handle case in handle().
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"iap\" to IapPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        let handle_at = core_swift.find("case \"iap\": return await IapPlugin.handle").expect("handle case");
        let stream_at = core_swift.find("case \"iap\": await IapPlugin.subscribe").expect("stream case");
        assert!(stream_at < handle_at, "stream case in subscribe(), handle case in handle()");
        // Play Billing Gradle dependency injected; NO gradle plugin / entitlements / permissions.
        let gradle = read(&root, "Android/app/build.gradle.kts");
        assert!(gradle.contains("com.android.billingclient:billing"), "billing dep injected");
        assert!(!gradle.contains("google-services"), "no gradle plugin for iap");
        let yml = read(&root, "iOS/project.yml");
        assert!(!yml.contains("entitlements:"), "iap adds no iOS entitlements");
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(!manifest.contains("uses-permission"), "iap declares no permissions (billing AAR merges its own)");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_bundled_push_firebase_only_injects_spm_package_and_registers_under_push() {
        let root = skeleton();
        add_at(&root, "push-firebase-only").unwrap();

        // iOS Firebase plugin + Android copies present.
        assert!(read(&root, "iOS/Sources/FirebasePushPlugin.swift").contains("FirebasePushPlugin"));
        assert!(read(&root, "Android/app/src/main/java/dev/mobiler/demo/PushPlugin.kt").contains("package dev.mobiler.demo"));
        // Registered under the cx name "push" in both shells (handle + stream cases).
        let core_kt = read(&root, "Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        assert!(core_kt.contains("\"push\" to PushPlugin(application),"));
        let core_swift = read(&root, "iOS/Sources/Core.swift");
        let handle_at = core_swift.find("case \"push\": return await FirebasePushPlugin.handle").expect("handle case");
        let stream_at = core_swift.find("case \"push\": await FirebasePushPlugin.subscribe").expect("stream case");
        assert!(stream_at < handle_at, "stream case in subscribe(), handle case in handle()");
        // The NEW spm_packages capability: Firebase remote package + the FirebaseMessaging product dep.
        let yml = read(&root, "iOS/project.yml");
        assert!(yml.contains("url: https://github.com/firebase/firebase-ios-sdk"), "Firebase SPM package injected");
        assert!(yml.contains("from: 11."), "Firebase version pinned");
        assert!(yml.contains("product: FirebaseMessaging"), "FirebaseMessaging product dependency injected");
        // Android FCM bits (same as push) + the iOS aps-environment entitlement.
        let gradle = read(&root, "Android/app/build.gradle.kts");
        assert!(gradle.contains("com.google.firebase:firebase-messaging"), "firebase dep");
        assert!(gradle.contains("id(\"com.google.gms.google-services\")"), "google-services gradle plugin");
        let manifest = read(&root, "Android/app/src/main/AndroidManifest.xml");
        assert!(manifest.contains("android:name=\".PushMessagingService\""), "FCM service");
        assert!(yml.contains("aps-environment"), "aps-environment entitlement");
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
        assert!(yml.contains("# mobiler:spm-packages"), "project.yml needs the spm-packages anchor (packages: block)");
        assert!(yml.contains("# mobiler:spm-dependencies"), "project.yml needs the spm-dependencies anchor (target deps)");
        assert!(
            t("Android/app/build.gradle.kts").contains("mobiler:gradle-deps"),
            "build.gradle.kts needs the mobiler:gradle-deps anchor"
        );
        assert!(
            t("Android/app/build.gradle.kts").contains("// mobiler:gradle-plugins"),
            "app build.gradle.kts needs the // mobiler:gradle-plugins anchor (apply a Gradle plugin)"
        );
        assert!(
            t("Android/build.gradle.kts").contains("// mobiler:gradle-plugins-classpath"),
            "project build.gradle.kts needs the // mobiler:gradle-plugins-classpath anchor"
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
