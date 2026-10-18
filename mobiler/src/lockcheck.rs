//! Two versions of a framework crate in an app's `Cargo.lock` (ADR-0056).
//!
//! An app that builds two `crux_core` or two `mobiler-core` versions fails with trait errors that
//! name neither version. The lock records every resolved package and, when a name has more than
//! one version, which version each dependent uses, so it can say who still pulls the old one.
//! This reads the lock only; it never resolves or touches the network.

use std::path::Path;

/// The crates an app must build exactly one version of.
const FRAMEWORK: [&str; 2] = ["crux_core", "mobiler-core"];

/// One message per framework crate with more than one version in `root/Cargo.lock` (empty when
/// each has one), or `None` when there is no lock or it isn't a lock cargo wrote.
pub fn check(root: &Path) -> Option<Vec<String>> {
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).ok()?;
    toml::from_str::<Lock>(&lock).ok()?;
    Some(duplicates(&lock))
}

/// Printed under a failed build: the duplicates, if any, as the likely cause.
pub fn build_hint(root: &Path) -> Option<String> {
    let found = check(root)?;
    (!found.is_empty()).then(|| {
        format!(
            "This app builds two versions of a framework crate, which fails with trait errors that \
             name neither version:\n  {}",
            found.join("\n  ")
        )
    })
}

pub fn duplicates(lock: &str) -> Vec<String> {
    let Ok(lock) = toml::from_str::<Lock>(lock) else { return Vec::new() };
    let mut out = Vec::new();
    for name in FRAMEWORK {
        let mut versions: Vec<&str> =
            lock.package.iter().filter(|p| p.name == name).map(|p| p.version.as_str()).collect();
        if versions.len() < 2 {
            continue;
        }
        versions.sort_by_key(|v| version_key(v));
        versions.dedup();
        let newest = versions[versions.len() - 1];
        for old in &versions[..versions.len() - 1] {
            let pulling: Vec<&Package> =
                lock.package.iter().filter(|p| p.dependencies.iter().any(|d| names(d, name, old))).collect();
            // An old framework crate pulls the old crux along with it; the app crate that pulls the
            // old framework crate is the one to change, so name app crates when there are any, and
            // a framework crate with its version (its own message names who pulls it).
            let app_only = pulling.iter().any(|p| !FRAMEWORK.contains(&p.name.as_str()));
            let mut users: Vec<String> = pulling
                .iter()
                .filter(|p| !app_only || !FRAMEWORK.contains(&p.name.as_str()))
                .map(|p| if FRAMEWORK.contains(&p.name.as_str()) { format!("{} {}", p.name, p.version) } else { p.name.clone() })
                .collect();
            users.sort();
            users.dedup();
            let who = if users.is_empty() { "something in the graph".to_string() } else { users.join(", ") };
            out.push(format!(
                "Cargo.lock has {name} {old} and {newest}: {old} is required by {who} — move it to {newest} (edit by hand)"
            ));
        }
    }
    out
}

/// Whether a lock dependency entry (`"name"`, `"name version"` or `"name version (source)"`)
/// is `name` at `version`.
fn names(entry: &str, name: &str, version: &str) -> bool {
    let mut parts = entry.split_whitespace();
    parts.next() == Some(name) && parts.next() == Some(version)
}

/// Orders `major.minor.patch` numerically; a pre-release suffix sorts by its numeric prefix.
fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-', '+']).map(|p| p.parse().unwrap_or(0)).collect()
}

#[derive(serde::Deserialize)]
struct Lock {
    #[serde(default)]
    package: Vec<Package>,
}

#[derive(serde::Deserialize)]
struct Package {
    name: String,
    version: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[cfg(test)]
mod test {
    use super::*;

    const TWO_CORES: &str = r#"
version = 4

[[package]]
name = "shared"
version = "0.1.0"
dependencies = ["crux_core 0.20.0", "mobile-admin", "mobiler-core 0.47.0"]

[[package]]
name = "mobile-admin"
version = "0.1.0"
dependencies = ["crux_core 0.18.0", "mobiler-core 0.46.0"]

[[package]]
name = "mobiler-core"
version = "0.46.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
dependencies = ["crux_core 0.18.0"]

[[package]]
name = "mobiler-core"
version = "0.47.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
dependencies = ["crux_core 0.20.0"]

[[package]]
name = "crux_core"
version = "0.18.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "crux_core"
version = "0.20.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;

    #[test]
    fn two_versions_name_the_crates_that_need_the_old_one() {
        let found = duplicates(TWO_CORES);
        assert_eq!(
            found,
            [
                "Cargo.lock has crux_core 0.18.0 and 0.20.0: 0.18.0 is required by mobile-admin — move it to 0.20.0 (edit by hand)",
                "Cargo.lock has mobiler-core 0.46.0 and 0.47.0: 0.46.0 is required by mobile-admin — move it to 0.47.0 (edit by hand)",
            ]
        );
    }

    #[test]
    fn one_version_each_is_quiet() {
        let one = "version = 4\n\n[[package]]\nname = \"shared\"\nversion = \"0.1.0\"\ndependencies = [\"crux_core\", \"mobiler-core\"]\n\n\
            [[package]]\nname = \"crux_core\"\nversion = \"0.20.0\"\n\n[[package]]\nname = \"mobiler-core\"\nversion = \"0.47.0\"\n";
        assert!(duplicates(one).is_empty());
        assert!(duplicates("not toml [[").is_empty());
        assert_eq!(check(Path::new("/nonexistent/mobiler-app")), None);
    }

    #[test]
    fn an_old_framework_crate_is_named_with_its_version() {
        // A dev-dependency of a path crate outside the workspace is not resolved, so only the old
        // mobiler-core pulls the old crux.
        let lock = TWO_CORES.replace("dependencies = [\"crux_core 0.18.0\", \"mobiler-core 0.46.0\"]", "dependencies = [\"mobiler-core 0.46.0\"]");
        assert_eq!(
            duplicates(&lock)[0],
            "Cargo.lock has crux_core 0.18.0 and 0.20.0: 0.18.0 is required by mobiler-core 0.46.0 — move it to 0.20.0 (edit by hand)"
        );
    }

    #[test]
    fn versions_order_numerically() {
        let lock = TWO_CORES.replace("0.18.0", "0.9.0");
        assert!(duplicates(&lock)[0].starts_with("Cargo.lock has crux_core 0.9.0 and 0.20.0"), "{:?}", duplicates(&lock));
    }
}
