//! `mobiler.toml` `[ios] local_network`: let the iOS app load plain HTTP from local addresses only
//! (`localhost`, `.local` names, LAN IPs) — never the open internet (ADR-0064). The template ships iOS's
//! `NSAppTransportSecurity` → `NSAllowsLocalNetworking` between `mobiler:local-network` markers in
//! `iOS/project.yml`, switched off; `local_network = true` switches it on, `false` (or no key) off.
//! Android needs nothing: its debug-only network config already allows the dev hosts (ADR-0038).

use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

use crate::deep_links::{toggle, PROJECT_YML};

pub(crate) const YML_BEGIN: &str = "# mobiler:local-network-begin";
pub(crate) const YML_END: &str = "# mobiler:local-network-end";
const YML_OFF: (&str, &str) = ("# off: ", "");

#[derive(Deserialize, Default)]
struct Manifest {
    ios: Option<Ios>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ios {
    #[serde(default)]
    local_network: bool,
}

#[derive(Default, Debug)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub warnings: Vec<String>,
    /// The config couldn't be read or applied, so nothing was written.
    pub failed: bool,
}

/// Sync `[ios] local_network` into `iOS/project.yml`. No `mobiler.toml` or no `[ios]` table: the block
/// is switched off. An app without iOS, or (when off) without the markers, is left alone.
pub fn sync(root: &Path) -> anyhow::Result<SyncReport> {
    let mut report = SyncReport::default();
    let Some(on) = read_enabled(root, &mut report)? else { return Ok(report) };
    let path = root.join(PROJECT_YML);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(e) => {
            report.warnings.push(format!("{PROJECT_YML}: can't read it ({e}) — nothing synced"));
            report.failed = true;
            return Ok(report);
        }
    };
    match toggle(&text, YML_BEGIN, YML_END, YML_OFF, on, "local-network") {
        Ok(Some(new)) if new != text => {
            crate::fsguard::write(root, &path, new)?;
            report.written.push(PROJECT_YML.to_string());
        }
        Ok(Some(_)) => {}
        Ok(None) if on => {
            report.warnings.push(format!(
                "{PROJECT_YML} has no local-network markers, so local networking can't be switched on — run `mobiler upgrade --apply` to get them"
            ));
            report.failed = true;
        }
        Ok(None) => {}
        Err(e) => {
            report.warnings.push(format!("{PROJECT_YML}: {e} — nothing synced"));
            report.failed = true;
        }
    }
    Ok(report)
}

/// `Some(on)`: `[ios] local_network`, false without the key, table or file. `None` when the table is
/// invalid (a warning).
fn read_enabled(root: &Path, report: &mut SyncReport) -> anyhow::Result<Option<bool>> {
    let path = root.join("mobiler.toml");
    if !path.exists() {
        return Ok(Some(false));
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    match toml::from_str::<Manifest>(&text) {
        Ok(m) => Ok(Some(m.ios.is_some_and(|i| i.local_network))),
        Err(e) => {
            report.warnings.push(format!("mobiler.toml [ios]: {} — nothing synced", e.message()));
            report.failed = true;
            Ok(None)
        }
    }
}

/// `mobiler local-network sync` — run the sync from the app root and print what happened. Fails (non-zero
/// exit) when the config couldn't be applied.
pub fn run_sync_cli() -> anyhow::Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let report = sync(&root)?;
    for w in &report.warnings {
        eprintln!("warning: local network: {w}");
    }
    for p in &report.written {
        println!("  wrote   {p}");
    }
    if report.failed {
        anyhow::bail!("[ios] local_network not applied (see the warnings above)");
    }
    if report.written.is_empty() {
        let mut ignored = SyncReport::default();
        let on = read_enabled(&root, &mut ignored)?.unwrap_or(false);
        println!("iOS local networking {}, up to date.", if on { "on" } else { "off" });
    }
    Ok(())
}

/// Called at the start of `mobiler build` / `dev` / `watch`: sync and print warnings, never fail.
pub fn sync_for_build(root: &Path) {
    match sync(root) {
        Ok(r) => {
            for w in r.warnings {
                eprintln!("warning: local network: {w}");
            }
        }
        Err(e) => eprintln!("warning: local network: {e:#} — not synced"),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn app(toml: Option<&str>) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("mob_localnet_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join(PROJECT_YML);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let tpl = crate::upgrade::TEMPLATES.get_file(PROJECT_YML).unwrap().contents_utf8().unwrap();
        fs::write(&path, tpl.replace("{{PACKAGE}}", "dev.test.net").replace("{{NAME}}", "Net")).unwrap();
        if let Some(t) = toml {
            fs::write(root.join("mobiler.toml"), t).unwrap();
        }
        root
    }

    fn yml(root: &Path) -> String {
        fs::read_to_string(root.join(PROJECT_YML)).unwrap()
    }

    /// The live (uncommented) lines between the markers.
    fn live_block(text: &str) -> Vec<String> {
        let b = text.find(YML_BEGIN).expect("begin marker");
        let e = text.find(YML_END).expect("end marker");
        text[b + YML_BEGIN.len()..e].lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect()
    }

    #[test]
    fn the_template_ships_local_networking_off() {
        let root = app(None);
        assert!(live_block(&yml(&root)).is_empty(), "nothing live between the markers by default");
        assert!(yml(&root).contains("NSAllowsLocalNetworking: true"), "the switched-off key is in the block");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn on_switches_on_only_local_networking_and_off_restores_the_template() {
        let root = app(Some("[ios]\nlocal_network = true\n"));
        let before = yml(&app(None));
        let r = sync(&root).unwrap();
        assert_eq!(r.written, vec![PROJECT_YML], "{r:?}");
        assert_eq!(live_block(&yml(&root)), vec!["NSAppTransportSecurity:", "NSAllowsLocalNetworking: true"]);
        assert!(!yml(&root).contains("NSAllowsArbitraryLoads"), "never arbitrary loads");
        fs::write(root.join("mobiler.toml"), "[ios]\nlocal_network = false\n").unwrap();
        sync(&root).unwrap();
        assert_eq!(yml(&root), before, "off restores the template exactly");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unknown_ios_key_or_missing_markers_refuse_and_write_nothing() {
        let root = app(Some("[ios]\nlocal_net = true\n"));
        let before = yml(&root);
        let r = sync(&root).unwrap();
        assert!(r.failed && r.written.is_empty(), "{r:?}");
        assert_eq!(yml(&root), before);
        fs::write(root.join("mobiler.toml"), "[ios]\nlocal_network = true\n").unwrap();
        fs::write(root.join(PROJECT_YML), before.replace(YML_BEGIN, "").replace(YML_END, "")).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.failed && r.warnings[0].contains("mobiler upgrade --apply"), "{r:?}");
        let _ = fs::remove_dir_all(&root);
    }
}
