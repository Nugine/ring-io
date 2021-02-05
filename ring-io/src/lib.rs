#![deny(single_use_lifetimes, missing_debug_implementations, clippy::all)]

#[cfg(target_pointer_width = "16")]
compile_error!("ring-io does not support this target");

mod sys;

mod utils;

mod cq;
mod reg;
mod ring;
mod sq;

pub use self::cq::CompletionQueue;
pub use self::reg::Registrar;
pub use self::ring::{Ring, RingBuilder};
pub use self::sq::SubmissionQueue;
