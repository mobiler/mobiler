#![allow(clippy::unsafe_derive_deserialize)]

pub mod ffi;

// The app logic lives in `barbershop-core` (shared with the web client). Re-export it so
// `App` (and the rest) are available at this crate's root — `ffi.rs` and `codegen` use it.
pub use barbershop_core::*;
pub use crux_core::Core;

#[cfg(feature = "uniffi")]
const _: () = assert!(
    uniffi::check_compatible_version("0.29.4"),
    "please use uniffi v0.29.4"
);
#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
