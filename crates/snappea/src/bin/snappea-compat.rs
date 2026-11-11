//! The `snappea-compat` binary: a drop-in replacement for upstream's `p` tool
//! ([`snappea::compat::run`]) on the native file system and console.

use std::io::{self, Write};

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut host = snappea::cli::NativeHost::new();
    let code = snappea::compat::run(&args, &mut host);
    let _ = io::stdout().flush();
    std::process::exit(code);
}
