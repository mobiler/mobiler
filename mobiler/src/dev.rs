use anyhow::{Context, Result, bail};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

pub fn run(no_install: bool, no_run: bool, device: Option<&str>) -> Result<()> {
    let project = Project::detect()?;
    let java_home = resolve_java_home();

    println!("Mobiler dev");
    println!(
        "  Project:   {} ({})",
        project.root.display(),
        project.application_id
    );
    if let Some(jh) = &java_home {
        println!("  JAVA_HOME: {jh}");
    }
    println!();

    pipeline(&project, java_home.as_deref(), no_install, no_run, device)
}

/// Run the full build pipeline. Reusable by `watch` between rebuilds.
pub fn pipeline(
    project: &Project,
    java_home: Option<&str>,
    no_install: bool,
    no_run: bool,
    device: Option<&str>,
) -> Result<()> {
    stage("Building Rust core (shared, uniffi)", || {
        run_capture(
            Command::new("cargo")
                .args(["build", "-p", "shared", "--features", "uniffi"])
                .current_dir(&project.root),
        )
    })?;

    stage("Generating Kotlin types + uniffi bindings", || {
        let _ = fs::remove_dir_all(project.root.join("Android/generated"));
        run_capture(
            Command::new("cargo")
                .args([
                    "run", "-p", "shared", "--bin", "codegen",
                    "--features", "codegen,facet_typegen",
                    "--", "--language", "kotlin",
                    "--output-dir", "Android/generated",
                ])
                .current_dir(&project.root),
        )
    })?;

    stage("Building Android APK (gradle :app:assembleDebug)", || {
        let mut cmd = Command::new(project.root.join("Android/gradlew"));
        cmd.args(["-p", "Android", "--no-daemon", ":app:assembleDebug"])
            .current_dir(&project.root);
        if let Some(jh) = java_home {
            cmd.env("JAVA_HOME", jh);
        }
        let out = run_capture(&mut cmd)?;
        // Gradle sometimes exits 0 even when a sub-task printed BUILD FAILED.
        // Parse stdout to be sure.
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !combined.contains("BUILD SUCCESSFUL") {
            bail!(
                "gradle did not report BUILD SUCCESSFUL\n--- last lines ---\n{}",
                tail(&combined, 30)
            );
        }
        Ok(out)
    })?;

    let apk = project.root.join("Android/app/build/outputs/apk/debug/app-debug.apk");
    if !apk.exists() {
        bail!("expected APK at {} but it was not produced", apk.display());
    }
    println!("  APK: {}", apk.display());

    if no_install {
        return Ok(());
    }

    let adb = locate_adb()?;
    let env_serial = env::var("ANDROID_SERIAL").ok();
    let Some(serial) = select_device(device, env_serial.as_deref(), &adb_devices(&adb)?)? else {
        println!();
        println!(
            "No Android device connected. Skipping install + launch.\n  \
             Boot the emulator first: emulator -avd <name>"
        );
        return Ok(());
    };
    println!("  Device:    {serial}");

    stage("Installing APK on device", || {
        run_capture(Command::new(&adb).args(["-s", &serial, "install", "-r"]).arg(&apk))
    })?;

    if no_run {
        return Ok(());
    }

    stage("Launching MainActivity", || {
        run_capture(Command::new(&adb).args([
            "-s",
            &serial,
            "shell",
            "am",
            "start",
            "-n",
            &format!("{}/.MainActivity", project.application_id),
        ]))
    })?;

    Ok(())
}

// -------------------- project detection --------------------

pub struct Project {
    pub root: PathBuf,
    pub application_id: String,
}

impl Project {
    pub fn detect() -> Result<Self> {
        let start = env::current_dir().context("could not read current directory")?;
        let root = walk_up_for_project_root(&start).ok_or_else(|| {
            anyhow::anyhow!(
                "could not find a Mobiler project root above {} \
                 (looked for a workspace Cargo.toml + an Android/ directory)",
                start.display()
            )
        })?;
        let application_id = parse_application_id(&root)
            .with_context(|| "could not determine applicationId from Android/app/build.gradle.kts")?;
        Ok(Self { root, application_id })
    }
}

fn walk_up_for_project_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(p) = cur {
        if looks_like_project_root(&p) {
            return Some(p);
        }
        cur = p.parent().map(Path::to_path_buf);
    }
    None
}

fn looks_like_project_root(p: &Path) -> bool {
    p.join("Cargo.toml").is_file()
        && p.join("Android").is_dir()
        && p.join("shared").is_dir()
}

fn parse_application_id(root: &Path) -> Result<String> {
    let path = root.join("Android/app/build.gradle.kts");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    // Match: applicationId = "com.foo.bar"
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("applicationId") {
            if let Some(value) = rest
                .trim_start()
                .strip_prefix('=')
                .map(str::trim_start)
                .and_then(|s| s.strip_prefix('"'))
                .and_then(|s| s.split('"').next())
            {
                return Ok(value.to_string());
            }
        }
    }
    bail!("no `applicationId = \"...\"` line found in {}", path.display());
}

// -------------------- env detection --------------------

pub fn resolve_java_home() -> Option<String> {
    // Prefer existing JAVA_HOME if it has a javac.
    if let Ok(jh) = env::var("JAVA_HOME") {
        if Path::new(&jh).join("bin/javac").exists() {
            return Some(jh);
        }
    }
    // If `javac` is on PATH and resolves to a JDK we can detect, use that.
    if let Ok(javac) = which::which("javac") {
        if let Some(jh) = javac.parent().and_then(Path::parent) {
            if jh.join("bin/javac").exists() {
                return Some(jh.display().to_string());
            }
        }
    }
    // Fall back to Android Studio's bundled JBR (the JDK we found via doctor).
    let candidates = [
        "/snap/android-studio/current/jbr",
        "/opt/android-studio/jbr",
    ];
    candidates
        .iter()
        .find(|p| Path::new(p).join("bin/javac").exists())
        .map(|s| (*s).to_string())
}

fn locate_adb() -> Result<PathBuf> {
    if let Ok(home) = env::var("ANDROID_HOME") {
        let p = Path::new(&home).join("platform-tools/adb");
        if p.exists() {
            return Ok(p);
        }
    }
    which::which("adb").context("`adb` not found (set ANDROID_HOME or put platform-tools on PATH)")
}

/// Which connected device to install on: an explicit `--device`, else `ANDROID_SERIAL`, else the
/// only one connected. With several connected and no choice made, the error lists the serials so
/// you can pick one. `Ok(None)` means nothing is connected (the caller skips install + launch).
fn select_device(
    explicit: Option<&str>,
    env_serial: Option<&str>,
    connected: &[String],
) -> Result<Option<String>> {
    if let Some(want) = explicit.or(env_serial) {
        if connected.iter().any(|d| d == want) {
            return Ok(Some(want.to_string()));
        }
        let source = if explicit.is_some() { "--device" } else { "ANDROID_SERIAL" };
        bail!(
            "{source} is `{want}`, which isn't connected. Connected: {}",
            if connected.is_empty() { "none".to_string() } else { connected.join(", ") }
        );
    }
    match connected {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        many => bail!(
            "{} devices connected: {}. Pick one with `--device <serial>` or ANDROID_SERIAL.",
            many.len(),
            many.join(", ")
        ),
    }
}

fn adb_devices(adb: &Path) -> Result<Vec<String>> {
    let out = Command::new(adb).arg("devices").output()?;
    if !out.status.success() {
        bail!("`adb devices` failed");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut result = Vec::new();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(serial) = line.split_whitespace().next() {
            if line.contains("\tdevice") {
                result.push(serial.to_string());
            }
        }
    }
    Ok(result)
}

// -------------------- helpers --------------------

fn stage<F>(label: &str, run: F) -> Result<()>
where
    F: FnOnce() -> Result<Output>,
{
    let started = Instant::now();
    let result = run();
    let elapsed = started.elapsed();
    match result {
        Ok(_) => {
            println!("[ok]   {label} ({:.1}s)", elapsed.as_secs_f64());
            Ok(())
        }
        Err(e) => {
            println!("[FAIL] {label} ({:.1}s)", elapsed.as_secs_f64());
            Err(e)
        }
    }
}

fn run_capture(cmd: &mut Command) -> Result<Output> {
    let cmd_str = format!("{cmd:?}");
    let out = cmd.output().with_context(|| format!("spawning {cmd_str}"))?;
    if !out.status.success() {
        bail!(
            "command failed (exit {:?})\n--- last lines ---\n{}",
            out.status.code(),
            tail(
                &format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
                30
            )
        );
    }
    Ok(out)
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::select_device;

    fn devices(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_devices_means_nothing_to_install_on() {
        assert_eq!(select_device(None, None, &[]).unwrap(), None);
        // Even an explicit choice can't help when nothing is connected.
        assert!(select_device(Some("emulator-5554"), None, &[]).is_err());
    }

    #[test]
    fn a_single_connected_device_is_used_without_asking() {
        let d = devices(&["emulator-5554"]);
        assert_eq!(select_device(None, None, &d).unwrap().as_deref(), Some("emulator-5554"));
    }

    #[test]
    fn several_devices_need_a_choice_and_the_error_lists_them() {
        let d = devices(&["emulator-5554", "emulator-5556"]);
        let err = select_device(None, None, &d).unwrap_err().to_string();
        assert!(err.contains("emulator-5554") && err.contains("emulator-5556"), "{err}");
        assert!(err.contains("--device"), "{err}");
    }

    #[test]
    fn the_flag_wins_over_the_environment_and_both_beat_the_single_device_rule() {
        let d = devices(&["emulator-5554", "emulator-5556"]);
        assert_eq!(select_device(Some("emulator-5556"), Some("emulator-5554"), &d).unwrap().as_deref(), Some("emulator-5556"));
        assert_eq!(select_device(None, Some("emulator-5554"), &d).unwrap().as_deref(), Some("emulator-5554"));
    }

    #[test]
    fn an_unknown_serial_is_an_error_that_names_what_is_connected() {
        let d = devices(&["emulator-5554"]);
        let err = select_device(Some("emulator-9999"), None, &d).unwrap_err().to_string();
        assert!(err.contains("emulator-9999") && err.contains("emulator-5554"), "{err}");
        // An ANDROID_SERIAL pointing at a device that went away is the same kind of mistake.
        assert!(select_device(None, Some("emulator-9999"), &d).is_err());
    }
}
