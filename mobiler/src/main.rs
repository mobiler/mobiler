use clap::{Parser, Subcommand};

mod build;
mod dev;
mod display_name;
mod doctor;
mod new;
mod plugin;
mod templating;
mod upgrade;
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
        /// The name users see (launcher label, system dialogs), e.g. "Appointments Admin".
        /// Defaults to the PascalCase project name. Change it later with `mobiler display-name`.
        #[arg(long)]
        display_name: Option<String>,
    },
    /// Build the Mobiler project, install, and launch (default: all three).
    Dev {
        /// Build only — don't install on a device.
        #[arg(long)]
        no_install: bool,
        /// Build (and install if applicable) but don't launch the activity.
        #[arg(long)]
        no_run: bool,
        /// Which connected device to install on (adb serial). Defaults to ANDROID_SERIAL, then the
        /// only connected device; with several connected, the error lists them.
        #[arg(long, value_name = "SERIAL")]
        device: Option<String>,
    },
    /// Watch shared/ and Android/ for changes; rebuild + reinstall + relaunch on each change.
    Watch {
        /// Build only on each change — don't install on a device.
        #[arg(long)]
        no_install: bool,
        /// Build (and install if applicable) but don't launch the activity.
        #[arg(long)]
        no_run: bool,
        /// Which connected device to install on (adb serial). Defaults to ANDROID_SERIAL, then the
        /// only connected device; with several connected, the error lists them.
        #[arg(long, value_name = "SERIAL")]
        device: Option<String>,
    },
    /// Build the native artifact only (no install/launch) — the cloud-buildable unit.
    Build {
        /// Target platform.
        #[arg(value_enum, default_value = "android")]
        platform: build::Platform,
    },
    /// Manage plugins (add a capability package to this app).
    Plugin {
        #[command(subcommand)]
        cmd: plugin::PluginCmd,
    },
    /// Show or set the user-visible app name (Android app_name + iOS CFBundleDisplayName).
    /// Project identifiers (Gradle root, Xcode target, theme) are left unchanged.
    DisplayName {
        /// The new display name. Omit to print the current one.
        name: Option<String>,
    },
    /// Update this app's generic native shells + `mobiler-core` dep to the CLI's templates.
    /// Non-destructive by default (writes `<file>.mobiler-new`); `--apply` overwrites in place
    /// (saving `<file>.mobiler-bak`). Never touches your Rust app code or plugin-patched files.
    Upgrade {
        /// Overwrite changed shell files in place instead of writing `.mobiler-new` (a
        /// `.mobiler-bak` of each is saved first).
        #[arg(long)]
        apply: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor::run(),
        Command::New { name, package, agentic, display_name } => match new::run(&name, package.as_deref(), agentic, display_name.as_deref()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::Dev { no_install, no_run, device } => match dev::run(no_install, no_run, device.as_deref()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::Watch { no_install, no_run, device } => match watch::run(no_install, no_run, device.as_deref()) {
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
        Command::Plugin { cmd } => match plugin::run(cmd) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::DisplayName { name } => match display_name::run(name.as_deref()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
        Command::Upgrade { apply } => match upgrade::run(apply) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}
