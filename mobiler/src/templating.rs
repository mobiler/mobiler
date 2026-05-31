//! Token substitution shared by `new` (scaffolding) and `plugin` (installing into an existing
//! app). Templates use `{{PACKAGE}}` / `{{PACKAGE_PATH}}` etc. in file *contents* and
//! `__PACKAGE_PATH__` in file *paths*.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// File extensions treated as binary — copied byte-for-byte, no templating.
const BINARY_EXTS: &[&str] = &["jar", "webp", "png", "ico"];
/// File names treated as binary regardless of extension.
const BINARY_NAMES: &[&str] = &["gradle-wrapper.jar", "gradlew.bat"];

/// Token substitutions applied to template paths + file contents.
pub(crate) struct Subs {
    /// User-visible project name (e.g. "Todos").
    pub name: String,
    /// Java/Kotlin package (e.g. "dev.mobiler.todos").
    pub package: String,
    /// Package as a path (e.g. "dev/mobiler/todos").
    pub package_path: String,
    /// Package + ".shared" — uniffi's output package.
    pub package_shared: String,
    /// Package + ".shared.types" — typegen's output package.
    pub package_shared_types: String,
    /// Installed NDK version, e.g. "30.0.14904198".
    pub ndk_version: String,
}

impl Subs {
    /// Build the package-derived substitutions from a reverse-DNS package id.
    pub(crate) fn from_package(package: String, name: String, ndk_version: String) -> Subs {
        Subs {
            package_path: package.replace('.', "/"),
            package_shared: format!("{package}.shared"),
            package_shared_types: format!("{package}.shared.types"),
            name,
            package,
            ndk_version,
        }
    }

    /// Recover substitutions from an already-scaffolded app on disk (for `plugin add`, which
    /// has no CLI args). Finds the package by locating `MainActivity.kt` under
    /// `Android/app/src/main/java/`.
    pub(crate) fn from_app_root(root: &Path) -> Result<Subs> {
        let java = root.join("Android/app/src/main/java");
        let main_activity = find_file(&java, "MainActivity.kt").ok_or_else(|| {
            anyhow::anyhow!(
                "couldn't find MainActivity.kt under {} — run this from a Mobiler app root",
                java.display()
            )
        })?;
        let pkg_dir = main_activity
            .parent()
            .and_then(|p| p.strip_prefix(&java).ok())
            .ok_or_else(|| anyhow::anyhow!("could not derive package from MainActivity.kt"))?;
        let package_path = pkg_dir.to_string_lossy().replace('\\', "/");
        if package_path.is_empty() {
            bail!("MainActivity.kt is not inside a package directory");
        }
        let package = package_path.replace('/', ".");
        // `name` is only needed for `{{NAME}}` (absent from plugin sources); best-effort.
        let name = read_app_name(root)
            .unwrap_or_else(|| package.rsplit('.').next().unwrap_or("App").to_string());
        // Recover the NDK pin already baked into the app so `upgrade` re-substitutes the same
        // value (otherwise `{{NDK_VERSION}}` would blank out and the gradle file would falsely
        // read as "changed"). Empty when absent — only `Android/shared/build.gradle.kts` uses it.
        let ndk_version = read_ndk_version(root).unwrap_or_default();
        Ok(Subs::from_package(package, name, ndk_version))
    }
}

/// Apply path-level transforms: replace `__PACKAGE_PATH__`, strip a trailing `.tmpl`.
pub(crate) fn templated_path(rel: &Path, subs: &Subs) -> PathBuf {
    let as_str = rel.to_string_lossy();
    let replaced = as_str.replace("__PACKAGE_PATH__", &subs.package_path);
    let replaced = replaced.strip_suffix(".tmpl").unwrap_or(&replaced);
    PathBuf::from(replaced)
}

pub(crate) fn substitute(raw: &str, subs: &Subs) -> String {
    // Order matters: longer tokens before `{{PACKAGE}}`, or a partial match mangles them.
    raw.replace("{{PACKAGE_SHARED_TYPES}}", &subs.package_shared_types)
        .replace("{{PACKAGE_SHARED}}", &subs.package_shared)
        .replace("{{PACKAGE_PATH}}", &subs.package_path)
        .replace("{{PACKAGE}}", &subs.package)
        .replace("{{NDK_VERSION}}", &subs.ndk_version)
        .replace("{{NAME}}", &subs.name)
}

pub(crate) fn is_binary(p: &Path) -> bool {
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        if BINARY_NAMES.contains(&name) {
            return true;
        }
    }
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| BINARY_EXTS.contains(&e))
        .unwrap_or(false)
}

/// Recursively find the first file named `name` under `dir`.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut subdirs = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    subdirs.iter().find_map(|d| find_file(d, name))
}

/// Best-effort app name from `Android/settings.gradle.kts` (`rootProject.name = "Todo"`).
fn read_app_name(root: &Path) -> Option<String> {
    read_quoted(root.join("Android/settings.gradle.kts"), "rootProject.name")
}

/// The NDK version pinned in `Android/shared/build.gradle.kts` (`ndkVersion = "…"`).
fn read_ndk_version(root: &Path) -> Option<String> {
    read_quoted(root.join("Android/shared/build.gradle.kts"), "ndkVersion")
}

/// First double-quoted value on the first line of `file` containing `key`.
fn read_quoted(file: PathBuf, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    let line = content.lines().find(|l| l.contains(key))?;
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}
