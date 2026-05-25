use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use crux_core::{
    cli::{BindgenArgsBuilder, bindgen},
    type_generation::facet::{Config, TypeRegistry},
};
use log::info;
use uniffi::deps::anyhow::Result;

use shared::{{NAME}}App;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Language {
    Kotlin,
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

    let typegen_app = TypeRegistry::new().register_app::<{{NAME}}App>()?.build()?;

    let name = match args.language {
        Language::Kotlin => "{{PACKAGE_SHARED_TYPES}}",
    };
    let config = Config::builder(name, &args.output_dir).build();

    match args.language {
        Language::Kotlin => {
            info!("Typegen for Kotlin");
            typegen_app.kotlin(&config)?;

            info!("Bindgen for Kotlin");
            let bindgen_args = BindgenArgsBuilder::default()
                .crate_name(env!("CARGO_PKG_NAME").to_string())
                .kotlin(&args.output_dir)
                .build()?;
            bindgen(&bindgen_args)?;
        }
    }

    Ok(())
}
