use clap::{Parser, Subcommand};

mod build;
mod dev;
mod doctor;
mod new;
mod watch;

#[derive(Parser)]
#[command(name = "mobiler", version, about = "Rust + Compose mobile framework CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that this host has everything needed to build Mobiler apps.
    Doctor,
    /// Scaffold a new Mobiler project.
    New {
        /// Project name (becomes the directory and Gradle root name).
        name: String,
        /// Android application/package id. Defaults to `dev.mobiler.<name>`.
        #[arg(long)]
        package: Option<String>,
        /// Write a `CLAUDE.md` agent guide so a coding agent (e.g. Claude Code) builds
        /// idiomatically. Bare `--agentic` = the generic primer; or pick a flavor:
        /// `shared-ui` (same UI on mobile + web) or `api` (reusable core + JSON API).
        #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "generic")]
        agentic: Option<new::AgenticGuide>,
    },
    /// Build the Mobiler project, install, and launch (default: all three).
    Dev {
        /// Build only — don't install on a device.
        #[arg(long)]
        no_install: bool,
        /// Build (and install if applicable) but don't launch the activity.
        #[arg(long)]
        no_run: bool,
    },
    /// Watch shared/ and Android/ for changes; rebuild + reinstall + relaunch on each change.
    Watch {
        /// Build only on each change — don't install on a device.
        #[arg(long)]
        no_install: bool,
        /// Build (and install if applicable) but don't launch the activity.
        #[arg(long)]
        no_run: bool,
    },
    /// Build the native artifact only (no install/launch) — the cloud-buildable unit.
    Build {
        /// Target platform.
        #[arg(value_enum, default_value = "android")]
        platform: build::Platform,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor::run(),
        Command::New { name, package, agentic } => match new::run(&name, package.as_deref(), agentic) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::Dev { no_install, no_run } => match dev::run(no_install, no_run) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::Watch { no_install, no_run } => match watch::run(no_install, no_run) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::Build { platform } => match build::run(platform) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}
