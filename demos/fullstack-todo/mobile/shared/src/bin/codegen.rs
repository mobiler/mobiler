use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use camino::Utf8PathBuf;
use cargo_metadata::{Metadata, MetadataCommand};
use crux_core::type_generation::facet::{Config, TypeRegistry};
use log::info;
use uniffi::deps::anyhow::{Context as _, Result, anyhow};

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

    // HttpOutcome rides inside PluginResponse.output as bincode, so register_app
    // cannot reach it by traversal — register it explicitly or the shells have no
    // generated encoder.
    let typegen_app = TypeRegistry::new()
        .register_app::<App>()?
        .register_type::<mobiler_core::HttpOutcome>()?
        .register_type::<mobiler_core::TransferEvent>()?
        .register_type::<mobiler_core::DeviceInfo>()?
        .register_type::<mobiler_core::Photo>()?
        // mobiler:codegen-types — insert above
        .build()?;

    match args.language {
        Language::Kotlin => {
            info!("Typegen for Kotlin");
            let config = Config::builder("dev.mobiler.mobile.shared.types", &args.output_dir).build();
            typegen_app.kotlin(&config)?;

            info!("Bindgen for Kotlin");
            generate_bindings(Language::Kotlin, &args.output_dir)?;
        }
        Language::Swift => {
            // Swift module name (no dots); the generic shell imports `SharedTypes`.
            info!("Typegen for Swift");
            let config = Config::builder("SharedTypes", &args.output_dir).build();
            typegen_app.swift(&config)?;

            info!("Bindgen for Swift");
            generate_bindings(Language::Swift, &args.output_dir)?;
        }
    }

    Ok(())
}

/// Generate the `UniFFI` bindings for this crate in library mode, from the library `cargo build` left
/// in the target directory. `shared/uniffi.toml` applies through the cargo-metadata config supplier.
fn generate_bindings(language: Language, out_dir: &Path) -> Result<()> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("running cargo metadata")?;
    let library = built_library(&metadata, env!("CARGO_PKG_NAME"))?;
    let out_dir = Utf8PathBuf::from_path_buf(out_dir.to_path_buf())
        .map_err(|p| anyhow!("path {} has non-unicode characters", p.display()))?;
    let config = uniffi::CargoMetadataConfigSupplier::from(metadata);
    match language {
        Language::Kotlin => uniffi::generate_bindings_library_mode(
            &library, None, &uniffi::KotlinBindingGenerator, &config, None, &out_dir, true,
        )
        .map(drop),
        Language::Swift => uniffi::generate_bindings_library_mode(
            &library, None, &uniffi::SwiftBindingGenerator, &config, None, &out_dir, true,
        )
        .map(drop),
    }
    .context("generating bindings")
}

/// The crate's library as `cargo build` left it: `<target>/debug/lib<name>.{rlib,dylib,a}`.
fn built_library(metadata: &Metadata, crate_name: &str) -> Result<Utf8PathBuf> {
    let lib_name = metadata
        .workspace_packages()
        .iter()
        .find(|package| package.name == crate_name)
        .ok_or_else(|| anyhow!("crate {crate_name:?} not found"))?
        .targets
        .iter()
        .find(|target| target.is_lib())
        .ok_or_else(|| anyhow!("crate {crate_name:?} has no lib target"))?
        .name
        .clone();
    let stem = metadata.target_directory.join(format!("debug/lib{lib_name}"));
    ["rlib", "dylib", "a"]
        .iter()
        .map(|ext| stem.with_extension(ext))
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("library {stem} not found; build the crate first"))
}
