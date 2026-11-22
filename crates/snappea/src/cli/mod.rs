//! The `snappea` command line ([`run`]), and the host it runs on natively. It runs the
//! commands of [`crate::driver`]; `snappea-compat`, upstream's `p`, is in [`crate::compat`].

mod command;
mod guide;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod observe;

pub use command::run;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{run_threads, NativeHost};
