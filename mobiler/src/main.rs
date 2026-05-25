use clap::{Parser, Subcommand};

mod add_widget;
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
    /// Add a new Widget variant + matching Compose Render arm.
    AddWidget {
        /// Variant name in PascalCase (e.g. Slider, Image, Snackbar).
        name: String,
        /// Struct fields as `name:Type`, repeatable. Omit for a unit variant.
        #[arg(long = "field", value_name = "name:Type")]
        fields: Vec<String>,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor::run(),
        Command::New { name, package } => match new::run(&name, package.as_deref()) {
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
        Command::AddWidget { name, fields } => match add_widget::run(&name, &fields) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}
