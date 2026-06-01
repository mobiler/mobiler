use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use crux_core::{
    cli::{BindgenArgsBuilder, bindgen},
    type_generation::facet::{Config, TypeRegistry},
};
use log::info;
use uniffi::deps::anyhow::Result;

use shared::App;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Language {
    Kotlin,
    Swift,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, value_enum)]
    language: Language,
    #[arg(short, long)]
    output_dir: PathBuf,
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let args = Args::parse();

    let typegen_app = TypeRegistry::new().register_app::<App>()?.build()?;

    match args.language {
        Language::Kotlin => {
            info!("Typegen for Kotlin");
            let config = Config::builder("dev.mobiler.barbershop.shared.types", &args.output_dir).build();
            typegen_app.kotlin(&config)?;

            info!("Bindgen for Kotlin");
            let bindgen_args = BindgenArgsBuilder::default()
                .crate_name(env!("CARGO_PKG_NAME").to_string())
                .kotlin(&args.output_dir)
                .build()?;
            bindgen(&bindgen_args)?;
        }
        Language::Swift => {
            // Swift module name (no dots); the generic shell imports `SharedTypes`.
            info!("Typegen for Swift");
            let config = Config::builder("SharedTypes", &args.output_dir).build();
            typegen_app.swift(&config)?;

            info!("Bindgen for Swift");
            let bindgen_args = BindgenArgsBuilder::default()
                .crate_name(env!("CARGO_PKG_NAME").to_string())
                .swift(&args.output_dir)
                .build()?;
            bindgen(&bindgen_args)?;
        }
    }

    Ok(())
}
