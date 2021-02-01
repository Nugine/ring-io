#![deny(single_use_lifetimes, missing_debug_implementations, clippy::all)]

#[cfg(target_pointer_width = "16")]
compile_error!("ring-io does not support this target");

pub mod sys;

mod utils;
