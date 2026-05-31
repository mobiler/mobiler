use crate::templating::{Subs, is_binary, substitute, templated_path};
use anyhow::{Context, Result, anyhow, bail};
use include_dir::{Dir, include_dir};
use std::fs;
use std::path::Path;

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// The Mobiler agent guide, written to the project as `CLAUDE.md` by `--agentic` so a
/// coding agent (e.g. Claude Code) builds the app idiomatically. Kept outside `templates/`
/// so it's emitted only on request. The base primer always applies; a flavor appends
/// architecture-specific guidance.
const GUIDE_BASE: &str = include_str!("../agentic/CLAUDE.md");
const GUIDE_SHARED_UI: &str = include_str!("../agentic/shared-ui.md");
const GUIDE_API: &str = include_str!("../agentic/api.md");

/// Which agent guide `mobiler new --agentic [<flavor>]` emits.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum AgenticGuide {
    /// Just the generic Mobiler primer (what a bare `--agentic` emits).
    Generic,
    /// Primer + "same UI on mobile and web".
    #[value(name = "shared-ui")]
    SharedUi,
    /// Primer + "reusable core + JSON API".
    Api,
}

/// The full `CLAUDE.md` text for a flavor: the base primer plus a flavor appendix.
fn agentic_guide(flavor: AgenticGuide) -> String {
    match flavor {
        AgenticGuide::Generic => GUIDE_BASE.to_string(),
        AgenticGuide::SharedUi => format!("{GUIDE_BASE}{GUIDE_SHARED_UI}"),
        AgenticGuide::Api => format!("{GUIDE_BASE}{GUIDE_API}"),
    }
}

/// Fallback NDK version pin when none is detectable. Update when bumping the framework's target NDK.
const FALLBACK_NDK_VERSION: &str = "30.0.14904198";

pub fn run(raw_name: &str, package: Option<&str>, agentic: Option<AgenticGuide>) -> Result<()> {
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

    let subs = Subs::from_package(package, display_name.clone(), ndk_version.clone());

    let mut written = 0usize;
    write_dir(&TEMPLATES, &out_dir, &subs, &mut written)?;
    make_gradlew_executable(&out_dir)?;
    // Stamp the generating CLI version so `mobiler upgrade` (and future drift checks) have a baseline.
    crate::upgrade::write_version_stamp(&out_dir).context("writing version stamp")?;
    written += 1;
    if let Some(sdk_dir) = android_home.as_deref() {
        write_local_properties(&out_dir, sdk_dir)?;
        written += 1;
    }
    if let Some(flavor) = agentic {
        fs::write(out_dir.join("CLAUDE.md"), agentic_guide(flavor)).context("writing CLAUDE.md")?;
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
    if agentic.is_some() {
        println!();
        println!("Wrote CLAUDE.md — a coding agent (e.g. Claude Code) will use it to build idiomatically.");
    }
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

fn detect_ndk_version(android_home: Option<&str>) -> Option<String> {
    let home = android_home?;
    let ndk_dir = Path::new(home).join("ndk");
    fs::read_dir(ndk_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .max()
}

fn write_local_properties(out_dir: &Path, sdk_dir: &str) -> Result<()> {
    let path = out_dir.join("Android/local.properties");
    fs::write(&path, format!("sdk.dir={sdk_dir}\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
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
        bail!("project name `{trimmed}` must contain only [a-zA-Z0-9_-]; got an invalid character");
    }
    if !trimmed.chars().next().unwrap().is_ascii_alphabetic() {
        bail!("project name must start with a letter");
    }
    Ok(trimmed.to_string())
}

/// Convert kebab-case / snake_case / lowercase project name into PascalCase.
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

fn default_package(name: &str) -> String {
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
            return Err(anyhow!("package segment `{part}` may contain only [a-zA-Z0-9_]"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::{
        AgenticGuide, agentic_guide, default_package, display_name_from, sanitize_project_name,
        validate_package,
    };
    use crate::templating::{Subs, is_binary, substitute, templated_path};
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
        Subs::from_package("dev.mobiler.todos".into(), "Todos".into(), "30.0.14904198".into())
    }

    #[test]
    fn templated_path_strips_tmpl_suffix() {
        let s = subs();
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
        assert_eq!(templated_path(Path::new("shared/src/app.rs"), &s), PathBuf::from("shared/src/app.rs"));
    }

    #[test]
    fn substitute_replaces_every_placeholder_in_specificity_order() {
        let s = subs();
        let raw = "p={{PACKAGE}} pst={{PACKAGE_SHARED_TYPES}} ps={{PACKAGE_SHARED}} \
                   pp={{PACKAGE_PATH}} n={{NAME}} ndk={{NDK_VERSION}}";
        let out = substitute(raw, &s);
        assert_eq!(
            out,
            "p=dev.mobiler.todos pst=dev.mobiler.todos.shared.types ps=dev.mobiler.todos.shared \
             pp=dev/mobiler/todos n=Todos ndk=30.0.14904198"
        );
        assert!(!out.contains("{{"), "no placeholder should be left behind");
    }

    #[test]
    fn sanitize_project_name_accepts_valid_and_rejects_bad() {
        assert_eq!(sanitize_project_name("  my-app ").unwrap(), "my-app");
        assert_eq!(sanitize_project_name("Counter").unwrap(), "Counter");
        assert!(sanitize_project_name("").is_err());
        assert!(sanitize_project_name("   ").is_err());
        assert!(sanitize_project_name("1app").is_err());
        assert!(sanitize_project_name("my app").is_err());
        assert!(sanitize_project_name("my.app").is_err());
    }

    #[test]
    fn validate_package_enforces_segments_and_chars() {
        assert!(validate_package("dev.mobiler.todos").is_ok());
        assert!(validate_package("dev.example").is_ok());
        assert!(validate_package("").is_err());
        assert!(validate_package("single").is_err());
        assert!(validate_package("dev..todos").is_err());
        assert!(validate_package("dev.1bad").is_err());
        assert!(validate_package("dev.bad-seg").is_err());
    }

    #[test]
    fn default_package_lowercases_and_underscores_hyphens() {
        assert_eq!(default_package("Todos"), "dev.mobiler.todos");
        assert_eq!(default_package("my-app"), "dev.mobiler.my_app");
        assert_eq!(default_package("Counter"), "dev.mobiler.counter");
    }

    #[test]
    fn is_binary_by_extension_and_by_name() {
        assert!(is_binary(Path::new("app/src/main/res/icon.png")));
        assert!(is_binary(Path::new("libs/foo.jar")));
        assert!(is_binary(Path::new("Android/gradlew.bat")));
        assert!(is_binary(Path::new("gradle/wrapper/gradle-wrapper.jar")));
        assert!(!is_binary(Path::new("shared/src/app.rs")));
        assert!(!is_binary(Path::new("Android/app/build.gradle.kts")));
    }

    #[test]
    fn agentic_guides_compose_base_plus_flavor() {
        let base = agentic_guide(AgenticGuide::Generic);
        assert!(base.contains("MobilerApp"), "base explains the core trait");
        assert!(base.contains("widget vocabulary"), "base lists the UI builder vocabulary");
        assert!(base.contains("cx."), "base covers capabilities");
        assert!(base.len() > 1500);
        assert!(!base.contains("mobiler_web"), "base must not assume a web target");
        assert!(!base.to_lowercase().contains("trunk"), "base must not mention the web toolchain");

        let shared = agentic_guide(AgenticGuide::SharedUi);
        assert!(shared.starts_with(&base), "flavor guides begin with the base primer");
        assert!(
            shared.contains("same UI on mobile and web") && shared.contains("mobiler_web"),
            "shared-ui adds the web target"
        );

        let api = agentic_guide(AgenticGuide::Api);
        assert!(api.starts_with(&base));
        assert!(api.contains("reusable core + JSON API") && api.contains("SQLx"), "api appendix present");
    }
}
