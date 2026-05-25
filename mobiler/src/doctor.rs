use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Outcome of a single check.
enum Status {
    /// Check passed. `detail` shows the discovered value (e.g. "stable-1.95.0").
    Ok { detail: String },
    /// Check passed but with a caveat the user might care about.
    Warn { detail: String, message: String },
    /// Check failed. `fix` is a copy-pasteable command or instruction.
    Fail { detail: String, fix: String },
    /// Check was skipped (e.g. Linux-only check on macOS).
    Skip { reason: String },
}

struct Check {
    name: &'static str,
    status: Status,
}

pub fn run() -> ExitCode {
    println!("Mobiler doctor -- checking your dev environment");
    println!("{}", "-".repeat(70));

    let checks = vec![
        check("rustup installed", rustup_installed()),
        check("Active Rust toolchain", active_toolchain()),
        check("Android Rust targets", android_rust_targets()),
        check("ANDROID_HOME", android_home()),
        check("Android SDK structure", android_sdk_structure()),
        check("cmdline-tools (sdkmanager)", cmdline_tools()),
        check("Android NDK", ndk_installed()),
        check("ANDROID_NDK_HOME", ndk_home()),
        check("Java compiler (javac)", javac()),
        check("KVM device (/dev/kvm)", kvm_device()),
        check("kvm group membership", kvm_group()),
        check("adb", adb_available()),
        check("emulator binary", emulator_binary()),
        check("AVDs configured", avds_configured()),
    ];

    let name_width = checks.iter().map(|c| c.name.len()).max().unwrap_or(20);
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    for c in &checks {
        let (tag, detail) = match &c.status {
            Status::Ok { detail } => ("[ok]  ", detail.clone()),
            Status::Warn { detail, message } => {
                warnings.push((c.name, message.clone()));
                ("[WARN]", detail.clone())
            }
            Status::Fail { detail, fix } => {
                failures.push((c.name, fix.clone()));
                ("[FAIL]", detail.clone())
            }
            Status::Skip { reason } => ("[skip]", reason.clone()),
        };
        println!("  {tag}  {:<width$}  {}", c.name, detail, width = name_width);
    }

    println!("{}", "-".repeat(70));

    if !warnings.is_empty() {
        println!("\nWarnings:");
        for (name, msg) in &warnings {
            println!("  {name}: {msg}");
        }
    }

    if failures.is_empty() {
        println!(
            "\nAll required checks passed. You're good to build Mobiler apps."
        );
        ExitCode::SUCCESS
    } else {
        println!("\n{} check(s) failed. Fix instructions:", failures.len());
        for (name, fix) in &failures {
            println!("\n  {name}:");
            for line in fix.lines() {
                println!("    {line}");
            }
        }
        ExitCode::FAILURE
    }
}

fn check(name: &'static str, status: Status) -> Check {
    Check { name, status }
}

// -------------------- individual checks --------------------

fn rustup_installed() -> Status {
    match which::which("rustup") {
        Ok(path) => match Command::new(&path).arg("--version").output() {
            Ok(out) if out.status.success() => Status::Ok {
                detail: String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("rustup")
                    .to_string(),
            },
            _ => Status::Fail {
                detail: format!("{} present but errored", path.display()),
                fix: "Reinstall rustup: https://rustup.rs".into(),
            },
        },
        Err(_) => Status::Fail {
            detail: "not found on PATH".into(),
            fix: "Install rustup:\n  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
                .into(),
        },
    }
}

fn active_toolchain() -> Status {
    let Ok(out) = Command::new("rustup").args(["show", "active-toolchain"]).output() else {
        return Status::Fail {
            detail: "rustup not invokable".into(),
            fix: "Resolve the rustup check above first.".into(),
        };
    };
    if !out.status.success() {
        return Status::Fail {
            detail: "rustup show failed".into(),
            fix: "Run: rustup default stable".into(),
        };
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Status::Ok { detail: s }
}

fn android_rust_targets() -> Status {
    let Ok(out) = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    else {
        return Status::Fail {
            detail: "rustup target list failed".into(),
            fix: "Resolve the rustup check above first.".into(),
        };
    };
    let installed: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap_or("")
        .lines()
        .collect();
    let needed = ["x86_64-linux-android", "aarch64-linux-android"];
    let missing: Vec<&&str> = needed.iter().filter(|t| !installed.contains(t)).collect();
    if missing.is_empty() {
        Status::Ok {
            detail: needed.join(", "),
        }
    } else {
        Status::Fail {
            detail: format!("missing: {}", missing.iter().copied().copied().collect::<Vec<_>>().join(", ")),
            fix: format!(
                "rustup target add {}",
                missing.iter().copied().copied().collect::<Vec<_>>().join(" ")
            ),
        }
    }
}

fn android_home() -> Status {
    let Ok(home) = env::var("ANDROID_HOME") else {
        return Status::Fail {
            detail: "not set".into(),
            fix: "Add to your shell rc (and re-source it):\n  export ANDROID_HOME=\"$HOME/Android/Sdk\""
                .into(),
        };
    };
    if Path::new(&home).is_dir() {
        Status::Ok { detail: home }
    } else {
        Status::Fail {
            detail: format!("set to {home}, but directory missing"),
            fix: "Install Android Studio (which creates ~/Android/Sdk), or correct ANDROID_HOME.".into(),
        }
    }
}

fn android_sdk_structure() -> Status {
    let Ok(home) = env::var("ANDROID_HOME") else {
        return Status::Skip {
            reason: "ANDROID_HOME unset".into(),
        };
    };
    let needed = ["platform-tools", "build-tools", "platforms"];
    let missing: Vec<&str> = needed
        .iter()
        .copied()
        .filter(|d| !Path::new(&home).join(d).is_dir())
        .collect();
    if missing.is_empty() {
        Status::Ok {
            detail: needed.join(", "),
        }
    } else {
        Status::Fail {
            detail: format!("missing: {}", missing.join(", ")),
            fix: "Open Android Studio -> gear -> SDK Manager and install the missing components.".into(),
        }
    }
}

fn cmdline_tools() -> Status {
    let Ok(home) = env::var("ANDROID_HOME") else {
        return Status::Skip {
            reason: "ANDROID_HOME unset".into(),
        };
    };
    let path = Path::new(&home).join("cmdline-tools/latest/bin/sdkmanager");
    if path.exists() {
        Status::Ok {
            detail: path.display().to_string(),
        }
    } else {
        Status::Fail {
            detail: "not installed".into(),
            fix: "Android Studio -> gear -> SDK Manager -> SDK Tools tab -> tick \
                  \"Android SDK Command-line Tools (latest)\" -> Apply."
                .into(),
        }
    }
}

fn ndk_installed() -> Status {
    let Ok(home) = env::var("ANDROID_HOME") else {
        return Status::Skip {
            reason: "ANDROID_HOME unset".into(),
        };
    };
    let ndk_dir = Path::new(&home).join("ndk");
    if !ndk_dir.is_dir() {
        return Status::Fail {
            detail: format!("{} missing", ndk_dir.display()),
            fix: "Android Studio -> gear -> SDK Manager -> SDK Tools tab -> tick \
                  \"NDK (Side by side)\" -> Apply."
                .into(),
        };
    }
    let versions: Vec<String> = fs::read_dir(&ndk_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    if versions.is_empty() {
        Status::Fail {
            detail: "no NDK versions installed".into(),
            fix: "See the NDK install step above.".into(),
        }
    } else {
        Status::Ok {
            detail: versions.join(", "),
        }
    }
}

fn ndk_home() -> Status {
    match env::var("ANDROID_NDK_HOME") {
        Ok(p) if Path::new(&p).is_dir() => Status::Ok { detail: p },
        Ok(p) => Status::Fail {
            detail: format!("set to {p}, but directory missing"),
            fix: "Correct ANDROID_NDK_HOME to point at the version dir under $ANDROID_HOME/ndk/".into(),
        },
        Err(_) => {
            // Try to auto-suggest a value from the installed NDK directory.
            let suggested = env::var("ANDROID_HOME")
                .ok()
                .map(|h| Path::new(&h).join("ndk"))
                .and_then(|d| {
                    fs::read_dir(&d).ok().and_then(|it| {
                        it.filter_map(Result::ok)
                            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                            .map(|e| e.path())
                            .max()
                    })
                });
            let fix = match suggested {
                Some(p) => format!(
                    "Add to your shell rc:\n  export ANDROID_NDK_HOME=\"{}\"",
                    p.display()
                ),
                None => "Set ANDROID_NDK_HOME to your NDK install dir (e.g. $ANDROID_HOME/ndk/<version>).".into(),
            };
            Status::Fail {
                detail: "not set".into(),
                fix,
            }
        }
    }
}

fn javac() -> Status {
    // Prefer JAVA_HOME, then PATH, then auto-detect Studio's JBR.
    if let Ok(jh) = env::var("JAVA_HOME") {
        let javac = Path::new(&jh).join("bin/javac");
        if javac.exists() {
            if let Some(v) = run_version(&javac, "--version") {
                return Status::Ok {
                    detail: format!("{v} (JAVA_HOME)"),
                };
            }
        }
    }
    if let Ok(p) = which::which("javac") {
        if let Some(v) = run_version(&p, "--version") {
            return Status::Ok { detail: v };
        }
    }
    // Look for Studio's bundled JBR — the conventional Linux snap path.
    let candidates = [
        "/snap/android-studio/current/jbr/bin/javac",
        "/opt/android-studio/jbr/bin/javac",
    ];
    let resolved: Vec<PathBuf> = candidates
        .iter()
        .flat_map(|pat| glob_first(pat))
        .collect();
    // Also probe versioned snap path /snap/android-studio/<rev>/jbr.
    let snap_rev = glob_first("/snap/android-studio/")
        .and_then(|p| fs::read_dir(p).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name() != "current")
        .filter_map(|e| {
            let jc = e.path().join("jbr/bin/javac");
            jc.exists().then_some(jc)
        })
        .max();
    let mut all = resolved;
    if let Some(s) = snap_rev {
        all.push(s);
    }
    if let Some(jbr_javac) = all.into_iter().find(|p| p.exists()) {
        let jh = jbr_javac.parent().and_then(|p| p.parent());
        let v = run_version(&jbr_javac, "--version").unwrap_or_else(|| "unknown".into());
        return Status::Warn {
            detail: format!("{v} (Android Studio JBR)"),
            message: format!(
                "javac not on PATH and JAVA_HOME unset, but Studio's JBR is available. \
                 Add to your shell rc:\n    export JAVA_HOME=\"{}\"",
                jh.map(|p| p.display().to_string()).unwrap_or_default()
            ),
        };
    }
    Status::Fail {
        detail: "not found".into(),
        fix: "Ubuntu's openjdk-21-jre is JRE-only (no javac). Install a full JDK:\n  \
              sudo apt install openjdk-21-jdk\nor set JAVA_HOME to Android Studio's bundled JBR \
              (e.g. /snap/android-studio/current/jbr)."
            .into(),
    }
}

fn run_version(bin: &Path, flag: &str) -> Option<String> {
    let out = Command::new(bin).arg(flag).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // javac --version prints to stderr historically; check both.
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !stdout.is_empty() {
        return Some(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    (!stderr.is_empty()).then_some(stderr)
}

fn glob_first(literal_or_pattern: &str) -> Option<PathBuf> {
    let p = Path::new(literal_or_pattern);
    p.exists().then(|| p.to_path_buf())
}

fn kvm_device() -> Status {
    if !cfg!(target_os = "linux") {
        return Status::Skip {
            reason: "non-Linux host".into(),
        };
    }
    let dev = Path::new("/dev/kvm");
    if !dev.exists() {
        return Status::Fail {
            detail: "/dev/kvm missing".into(),
            fix: "Hardware virtualisation (KVM) is not enabled. \
                  Check BIOS settings, or install qemu-kvm: sudo apt install qemu-kvm".into(),
        };
    }
    match fs::metadata(dev) {
        Ok(_) if fs::File::open(dev).is_ok() => Status::Ok {
            detail: "present, readable".into(),
        },
        _ => Status::Fail {
            detail: "exists but not readable".into(),
            fix: "Make sure you're in the kvm group (see kvm group check below).".into(),
        },
    }
}

fn kvm_group() -> Status {
    if !cfg!(target_os = "linux") {
        return Status::Skip {
            reason: "non-Linux host".into(),
        };
    }
    let Ok(out) = Command::new("groups").output() else {
        return Status::Warn {
            detail: "could not run `groups`".into(),
            message: "skipped".into(),
        };
    };
    let groups = String::from_utf8_lossy(&out.stdout);
    if groups.split_whitespace().any(|g| g == "kvm") {
        Status::Ok { detail: "yes".into() }
    } else {
        Status::Fail {
            detail: "user not in kvm group".into(),
            fix: "sudo usermod -aG kvm $USER\n(then log out and back in to apply)".into(),
        }
    }
}

fn adb_available() -> Status {
    // Prefer ANDROID_HOME's platform-tools/adb so we don't rely on PATH ordering.
    if let Ok(home) = env::var("ANDROID_HOME") {
        let p = Path::new(&home).join("platform-tools/adb");
        if p.exists() {
            return Status::Ok {
                detail: p.display().to_string(),
            };
        }
    }
    match which::which("adb") {
        Ok(p) => Status::Ok {
            detail: p.display().to_string(),
        },
        Err(_) => Status::Fail {
            detail: "not found".into(),
            fix: "Install platform-tools via Android Studio SDK Manager, or add \
                  $ANDROID_HOME/platform-tools to PATH."
                .into(),
        },
    }
}

fn emulator_binary() -> Status {
    if let Ok(home) = env::var("ANDROID_HOME") {
        let p = Path::new(&home).join("emulator/emulator");
        if p.exists() {
            return Status::Ok {
                detail: p.display().to_string(),
            };
        }
    }
    Status::Fail {
        detail: "$ANDROID_HOME/emulator/emulator missing".into(),
        fix: "Android Studio -> gear -> SDK Manager -> SDK Tools tab -> tick \"Android Emulator\" -> Apply.".into(),
    }
}

fn avds_configured() -> Status {
    let avd_dir = match dirs_avd() {
        Some(p) => p,
        None => {
            return Status::Warn {
                detail: "could not determine ~/.android/avd".into(),
                message: "skipped".into(),
            }
        }
    };
    if !avd_dir.is_dir() {
        return Status::Warn {
            detail: "no AVD directory".into(),
            message: "Create one via Studio AVD Manager or `avdmanager create avd`.".into(),
        };
    }
    let names: Vec<String> = fs::read_dir(&avd_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix(".avd").map(str::to_string)
        })
        .collect();
    if names.is_empty() {
        Status::Warn {
            detail: "none".into(),
            message: "Create one via Studio AVD Manager or `avdmanager create avd`.".into(),
        }
    } else {
        Status::Ok {
            detail: names.join(", "),
        }
    }
}

fn dirs_avd() -> Option<PathBuf> {
    env::var_os("HOME").map(|h| Path::new(&h).join(".android/avd"))
}
