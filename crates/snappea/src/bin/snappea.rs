//! The `snappea` binary: [`snappea::cli::run`] on the native file system and console.

use std::io::{self, Write};
use std::process::ExitCode;

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Returns rather than calling `process::exit`: on Windows that is `ExitProcess`, which
// skips the C runtime's exit handlers, and the PGO profiler writes its profile in one.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut host = snappea::host::NativeHost::new();
    let code = snappea::cli::run(&args, &mut host);
    let _ = io::stdout().flush();
    // the low byte, as `process::exit` keeps on Unix; snappea's statuses are 0, 1 and 2
    ExitCode::from(code as u8)
}
