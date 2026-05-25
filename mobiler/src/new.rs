use anyhow::{Context, Result, anyhow, bail};
use include_dir::{Dir, include_dir};
use std::fs;
use std::path::{Path, PathBuf};

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// File extensions treated as binary — copied byte-for-byte, no templating.
const BINARY_EXTS: &[&str] = &["jar", "webp", "png", "ico"];

/// File names treated as binary regardless of extension.
const BINARY_NAMES: &[&str] = &["gradle-wrapper.jar", "gradlew.bat"];

struct Subs {
    /// User-visible project name (e.g. "Todos").
    name: String,
    /// Java/Kotlin package (e.g. "com.example.todos").
    package: String,
    /// Package as a path (e.g. "com/example/todos").
    package_path: String,
    /// Package + ".shared" — uniffi's output package.
    package_shared: String,
    /// Package + ".shared.types" — typegen's output package.
    package_shared_types: String,
    /// Installed NDK version, e.g. "30.0.14904198". Detected from $ANDROID_HOME/ndk/.
    ndk_version: String,
}

/// Fallback NDK version pin when none is detectable. Update when bumping the framework's target NDK.
const FALLBACK_NDK_VERSION: &str = "30.0.14904198";

pub fn run(raw_name: &str, package: Option<&str>) -> Result<()> {
    let name = sanitize_project_name(raw_name)?;
    let display_name = display_name_from(&name);
    let package = package
        .map(str::to_string)
        .unwrap_or_else(|| default_package(&name));
    validate_package(&package)?;

    let out_dir = std::env::current_dir()
        .context("could not read current directory")?
        .join(&name);

    if out_dir.exists() {
        bail!("destination `{}` already exists", out_dir.display());
    }

    let android_home = std::env::var("ANDROID_HOME").ok();
    let ndk_version = detect_ndk_version(android_home.as_deref())
        .unwrap_or_else(|| FALLBACK_NDK_VERSION.to_string());

    let subs = Subs {
        name: display_name.clone(),
        package_path: package.replace('.', "/"),
        package_shared: format!("{package}.shared"),
        package_shared_types: format!("{package}.shared.types"),
        package,
        ndk_version: ndk_version.clone(),
    };

    let mut written = 0usize;
    write_dir(&TEMPLATES, &out_dir, &subs, &mut written)?;
    make_gradlew_executable(&out_dir)?;
    if let Some(sdk_dir) = android_home.as_deref() {
        write_local_properties(&out_dir, sdk_dir)?;
        written += 1;
    }

    println!(
        "Created Mobiler app at {} ({} files)",
        out_dir.display(),
        written
    );
    println!();
    println!("  Rust core:      shared/");
    println!("  Android shell:  Android/");
    println!("  Package:        {}", subs.package);
    println!();
    println!("Next steps:");
    println!("  cd {name}");
    println!("  cargo test                                # run Rust core tests");
    println!("  cd Android");
    println!("  ./gradlew :app:assembleDebug              # build APK");
    println!("  adb install -r app/build/outputs/apk/debug/app-debug.apk");
    Ok(())
}

fn write_dir(dir: &Dir<'_>, out_root: &Path, subs: &Subs, written: &mut usize) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => write_dir(sub, out_root, subs, written)?,
            include_dir::DirEntry::File(file) => {
                let rel = templated_path(file.path(), subs);
                let dst = out_root.join(&rel);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating dir {}", parent.display()))?;
                }
                if is_binary(file.path()) {
                    fs::write(&dst, file.contents())
                        .with_context(|| format!("writing {}", dst.display()))?;
                } else {
                    let raw = std::str::from_utf8(file.contents())
                        .with_context(|| format!("template {} is not UTF-8", file.path().display()))?;
                    fs::write(&dst, substitute(raw, subs))
                        .with_context(|| format!("writing {}", dst.display()))?;
                }
                *written += 1;
            }
        }
    }
    Ok(())
}

/// Apply path-level transforms:
/// - `__PACKAGE_PATH__` is replaced with the package's directory form.
/// - A trailing `.tmpl` is stripped. Template manifests are stored as
///   `Cargo.toml.tmpl` so `cargo package` doesn't treat `templates/` as a nested
///   package and drop it from the published crate; they scaffold back to
///   `Cargo.toml`.
fn templated_path(rel: &Path, subs: &Subs) -> PathBuf {
    let as_str = rel.to_string_lossy();
    let replaced = as_str.replace("__PACKAGE_PATH__", &subs.package_path);
    let replaced = replaced.strip_suffix(".tmpl").unwrap_or(&replaced);
    PathBuf::from(replaced)
}

fn substitute(raw: &str, subs: &Subs) -> String {
    raw.replace("{{PACKAGE_SHARED_TYPES}}", &subs.package_shared_types)
        .replace("{{PACKAGE_SHARED}}", &subs.package_shared)
        .replace("{{PACKAGE_PATH}}", &subs.package_path)
        .replace("{{PACKAGE}}", &subs.package)
        .replace("{{NDK_VERSION}}", &subs.ndk_version)
        .replace("{{NAME}}", &subs.name)
}

fn detect_ndk_version(android_home: Option<&str>) -> Option<String> {
    let home = android_home?;
    let ndk_dir = Path::new(home).join("ndk");
    let max = fs::read_dir(ndk_dir).ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .max();
    max
}

fn write_local_properties(out_dir: &Path, sdk_dir: &str) -> Result<()> {
    let path = out_dir.join("Android/local.properties");
    fs::write(&path, format!("sdk.dir={sdk_dir}\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn is_binary(p: &Path) -> bool {
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

#[cfg(unix)]
fn make_gradlew_executable(out_dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let gradlew = out_dir.join("Android/gradlew");
    if gradlew.exists() {
        let mut perms = fs::metadata(&gradlew)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&gradlew, perms)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_gradlew_executable(_out_dir: &Path) -> Result<()> {
    Ok(())
}

// -------------------- input validation --------------------

fn sanitize_project_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("project name must not be empty");
    }
    let valid = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        bail!(
            "project name `{trimmed}` must contain only [a-zA-Z0-9_-]; got an invalid character"
        );
    }
    if !trimmed.chars().next().unwrap().is_ascii_alphabetic() {
        bail!("project name must start with a letter");
    }
    Ok(trimmed.to_string())
}

/// Convert kebab-case / snake_case / lowercase project name into PascalCase.
/// Examples: "mobiler-test" -> "MobilerTest", "todos" -> "Todos", "my_app" -> "MyApp".
fn display_name_from(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in name.chars() {
        if c == '-' || c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod test {
    use super::{Subs, display_name_from, templated_path};
    use std::path::{Path, PathBuf};

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(display_name_from("todos"), "Todos");
        assert_eq!(display_name_from("mobiler-test"), "MobilerTest");
        assert_eq!(display_name_from("my_app"), "MyApp");
        assert_eq!(display_name_from("Counter"), "Counter");
        assert_eq!(display_name_from("foo-bar-baz"), "FooBarBaz");
    }

    fn subs() -> Subs {
        Subs {
            name: "Todos".into(),
            package: "dev.mobiler.todos".into(),
            package_path: "dev/mobiler/todos".into(),
            package_shared: "dev.mobiler.todos.shared".into(),
            package_shared_types: "dev.mobiler.todos.shared.types".into(),
            ndk_version: "30.0.14904198".into(),
        }
    }

    #[test]
    fn templated_path_strips_tmpl_suffix() {
        let s = subs();
        // Template manifests scaffold back to real manifests.
        assert_eq!(templated_path(Path::new("Cargo.toml.tmpl"), &s), PathBuf::from("Cargo.toml"));
        assert_eq!(
            templated_path(Path::new("shared/Cargo.toml.tmpl"), &s),
            PathBuf::from("shared/Cargo.toml")
        );
    }

    #[test]
    fn templated_path_expands_package_path() {
        let s = subs();
        assert_eq!(
            templated_path(Path::new("Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt"), &s),
            PathBuf::from("Android/app/src/main/java/dev/mobiler/todos/MainActivity.kt")
        );
        // Non-template files are left untouched.
        assert_eq!(templated_path(Path::new("shared/src/app.rs"), &s), PathBuf::from("shared/src/app.rs"));
    }
}

fn default_package(name: &str) -> String {
    // Package names must contain only [a-z0-9_], so lowercase + replace hyphens.
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c == '-' { '_' } else { c })
        .collect();
    format!("dev.mobiler.{sanitized}")
}

fn validate_package(pkg: &str) -> Result<()> {
    if pkg.is_empty() {
        bail!("package must not be empty");
    }
    let parts: Vec<&str> = pkg.split('.').collect();
    if parts.len() < 2 {
        bail!("package must have at least two dot-separated segments (e.g. `dev.example`)");
    }
    for part in parts {
        if part.is_empty() {
            bail!("package `{pkg}` has an empty segment");
        }
        if !part.chars().next().unwrap().is_ascii_alphabetic() {
            bail!("package segment `{part}` must start with a letter");
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(anyhow!(
                "package segment `{part}` may contain only [a-zA-Z0-9_]"
            ));
        }
    }
    Ok(())
}
