//! Drop-in compatibility with the upstream P command line tools (`snappea-compat`).
//!
//! [`run`] is upstream's `p` tool: `p check` and `p merge-scenario-coverage` from
//! `PCommandLine`, and `p observe` from PObserve, with the same options, help text, console
//! messages, output files and exit codes. There is no `p compile`: the commands read the P
//! sources directly.
//!
//! This module only parses upstream's command lines; the commands themselves are
//! [`crate::driver`], shared with `snappea`. Nothing else in the crate depends on it. Known
//! differences from upstream are listed in `docs/compatibility.md`.

mod argparse;
mod check;
pub mod observe;

use std::path::Path;

use crate::driver::scenario;
use crate::host::{full_path, Color, Host, HostExt};

/// The version reported by `p --version`.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (snappea)");

/// `CommandLine.Main`: runs the `p` tool with `args` (without the program name) and returns
/// the process exit code.
pub fn run(args: &[String], host: &mut dyn Host) -> i32 {
    let code = match args.split_first() {
        None => {
            print_command_help(host);
            0
        }
        Some((cmd, rest)) => match cmd.to_lowercase().as_str() {
            // PObserve is its own tool upstream: its output has no P tool banner
            "observe" => return observe::run_with(rest, host, Vec::new()),
            "check" => check::run(rest, host),
            "merge-scenario-coverage" => merge_scenario_coverage(rest, host),
            "version" | "-v" | "-version" | "--version" => {
                host.out_line(&format!("P version {}", VERSION));
                0
            }
            "help" | "-h" | "--help" => {
                print_command_help(host);
                0
            }
            _ => {
                host.out_colored_line(
                    &format!(
                        "Expected (check | observe | merge-scenario-coverage) as the command input but received `{}`",
                        cmd
                    ),
                    Color::Red,
                );
                print_command_help(host);
                0
            }
        },
    };
    host.out_line("~~ [PTool]: Thanks for using P! ~~");
    code
}

fn print_command_help(host: &mut dyn Host) {
    for l in [
        "================================================================================",
        "The P commandline tool supports the commands: check, observe and merge-scenario-coverage.\n",
        "usage:> p command options",
        "\t command : check                     run the P checker on the P sources",
        "\t           observe                   check recorded logs against spec monitors",
        "\t           merge-scenario-coverage   merge scenario coverage reports",
        "\t options:                             use `--help` or `-h` to learn more about the",
        "\t                                      corresponding command options",
        "\t -----------------------------------------------------------------------",
        "\t p check --help                       for P checker help",
        "\t p observe --help                     for P observe help",
        "================================================================================",
    ] {
        host.out_line(l);
    }
}

/// `p merge-scenario-coverage [directory]`.
fn merge_scenario_coverage(args: &[String], host: &mut dyn Host) -> i32 {
    // upstream has no help and looks for a directory named `--help`
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        host.out_line("usage: p merge-scenario-coverage [directory]");
        host.out_line("");
        host.out_line(&format!(
            "Merges the scenario coverage reports (*{}) under the directory, recursively, into one report. The directory defaults to the current one.",
            scenario::FILE_SUFFIX
        ));
        return 0;
    }
    let dir = args.first().cloned().unwrap_or_else(|| host.cwd().to_string_lossy().to_string());
    let full = full_path(&host.cwd(), Path::new(&dir));
    if !host.is_dir(&full) {
        host.out_colored_line(&format!("Directory not found: {}", dir), Color::Red);
        return 0;
    }
    scenario::merge_dir(host, &full, false);
    0
}
