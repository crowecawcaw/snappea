//! The `snappea` and `snappea-compat` command line tools.
//!
//! `snappea` ([`run`]) is snappea's own command line and `snappea-compat` upstream's `p`, option for
//! option. Both run the commands of [`crate::driver`]; `snappea` adds
//! what upstream lacks (parallel workers, time tracking) and options named for how snappea works.

mod command;
mod guide;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod observe;

pub use command::run;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{run_threads, NativeHost};
