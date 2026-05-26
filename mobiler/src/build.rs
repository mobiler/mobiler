use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::dev::{Project, pipeline, resolve_java_home};

/// Native build target.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Platform {
    Android,
    Ios,
}

/// Build the native artifact only — no install, no launch. This is the unit a
/// cloud build worker runs (`build-android.sh` / `build-ios.sh` capture the same
/// steps for off-CLI use).
pub fn run(platform: Platform) -> Result<()> {
    let project = Project::detect()?;
    match platform {
        Platform::Android => {
            let java_home = resolve_java_home();
            println!("Mobiler build (android)");
            println!("  Project:   {} ({})", project.root.display(), project.application_id);
            if let Some(jh) = &java_home {
                println!("  JAVA_HOME: {jh}");
            }
            println!();
            // `no_install = true` makes the pipeline stop right after the APK is
            // produced (and printed) — exactly a build, no device needed.
            pipeline(&project, java_home.as_deref(), true, true)
        }
        Platform::Ios => {
            // iOS builds run the reproducible script (macOS only). The CLI doesn't
            // scaffold an iOS shell yet, so guide the user if it's absent.
            let script = project.root.join("iOS/build-ios.sh");
            if !script.exists() {
                bail!(
                    "no iOS shell in this project (expected iOS/build-ios.sh).\n\
                     iOS support is in progress; `mobiler build android` works today."
                );
            }
            println!("Mobiler build (ios) — running iOS/build-ios.sh (macOS only)\n");
            let status = Command::new("bash")
                .arg(&script)
                .status()
                .context("failed to run iOS/build-ios.sh")?;
            if !status.success() {
                bail!("iOS build failed (see output above)");
            }
            Ok(())
        }
    }
}
