//! Option parsing shared by the `snappea` commands, and test case selection.

use super::*;

/// An option a command accepts.
pub(in crate::cli) struct Opt {
    pub(in crate::cli) long: &'static str,
    pub(in crate::cli) short: Option<char>,
    pub(in crate::cli) takes_value: bool,
}

pub(in crate::cli) const fn flag(long: &'static str, short: Option<char>) -> Opt {
    Opt { long, short, takes_value: false }
}

pub(in crate::cli) const fn value(long: &'static str, short: Option<char>) -> Opt {
    Opt { long, short, takes_value: true }
}

/// Parsed arguments: positionals, and options (long name, value) in order.
#[derive(Default)]
pub(in crate::cli) struct Args {
    pub positional: Vec<String>,
    pub options: Vec<(&'static str, String)>,
    pub help: bool,
}

impl Args {
    pub fn has(&self, long: &str) -> bool {
        self.options.iter().any(|(l, _)| *l == long)
    }
    /// The last value given for `long`.
    pub fn get(&self, long: &str) -> Option<&str> {
        self.options.iter().rev().find(|(l, _)| *l == long).map(|(_, v)| v.as_str())
    }
    /// Every value given for `long`, in order.
    pub fn all(&self, long: &str) -> Vec<&str> {
        self.options.iter().filter(|(l, _)| *l == long).map(|(_, v)| v.as_str()).collect()
    }
}

pub(in crate::cli) fn parse_args(args: &[String], spec: &[Opt]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    let mut only_positional = false;
    while let Some(a) = it.next() {
        if only_positional || a == "-" || !a.starts_with('-') {
            out.positional.push(a.clone());
            continue;
        }
        if a == "--" {
            only_positional = true;
            continue;
        }
        if a == "-h" || a == "--help" {
            out.help = true;
            continue;
        }
        let (opt, inline) = if let Some(long) = a.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            (spec.iter().chain(OUTPUT_OPTS).find(|o| o.long == name), inline)
        } else {
            // `-j 4` or `-j4`
            let mut chars = a[1..].chars();
            let c = chars.next();
            let rest = chars.as_str();
            let opt =
                spec.iter().chain(OUTPUT_OPTS).find(|o| o.short == c).filter(|o| rest.is_empty() || o.takes_value);
            (opt, (!rest.is_empty()).then(|| rest.to_string()))
        };
        let Some(opt) = opt else {
            let name = a.split('=').next().unwrap_or(a);
            let longs = spec.iter().chain(OUTPUT_OPTS).map(|o| o.long).filter(|l| !l.is_empty());
            return Err(match crate::syntax::closest(name.trim_start_matches('-'), longs) {
                Some(l) => format!("unknown option '{}'; did you mean '--{}'?", a, l),
                None => format!("unknown option '{}'", a),
            });
        };
        let v = match (opt.takes_value, inline) {
            (true, Some(v)) => v,
            (true, None) => it.next().cloned().ok_or_else(|| format!("'{}' needs a value", a))?,
            (false, None) => String::new(),
            (false, Some(_)) => return Err(format!("'--{}' takes no value", opt.long)),
        };
        out.options.push((opt.long, v));
    }
    Ok(out)
}

/// Options every command takes: how it prints its result.
pub(in crate::cli) const OUTPUT_OPTS: &[Opt] = &[value("format", None), flag("json", None), value("json-report", None)];

/// The output options, for help texts.
pub(in crate::cli) const OUTPUT_HELP: &str = "\
Output format:
      --format <F>          text or json. Default: text, or SNAPPEA_FORMAT if set.
      --json                Same as --format json
      --json-report <FILE>  Also write the JSON result to FILE
";

/// How a command prints its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum Format {
    Text,
    Json,
}

/// `--json` or `--format`; else `SNAPPEA_FORMAT`; else text.
pub(in crate::cli) fn output_format(a: &Args, host: &dyn Host) -> Result<Format, String> {
    let parse = |v: &str, from: &str| match v {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        _ => Err(format!("{} must be text or json, not '{}'", from, v)),
    };
    if a.has("json") {
        return match a.get("format") {
            Some(f) if f != "json" => Err(format!("'--json' and '--format {}' conflict", f)),
            _ => Ok(Format::Json),
        };
    }
    if let Some(f) = a.get("format") {
        return parse(f, "'--format'");
    }
    if let Some(f) = host.env_var("SNAPPEA_FORMAT").filter(|v| !v.is_empty()) {
        return parse(&f, "SNAPPEA_FORMAT");
    }
    Ok(Format::Text)
}

/// `--progress`: how `check` shows a running check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum ProgressMode {
    Auto,
    Off,
    Json,
}

pub(in crate::cli) fn progress_mode(a: &Args) -> Result<ProgressMode, String> {
    match a.get("progress") {
        None | Some("auto") => Ok(ProgressMode::Auto),
        Some("off") => Ok(ProgressMode::Off),
        Some("json") => Ok(ProgressMode::Json),
        Some(v) => Err(format!("'--progress' takes auto, off or json, not '{}'", v)),
    }
}

/// How progress is shown: with `auto`, a status line when stderr is a terminal, a line
/// every 30 seconds in CI, and nothing otherwise or with `--verbose` (which logs every
/// schedule instead).
pub(in crate::cli) fn progress_view(a: &Args, host: &dyn Host) -> Option<ProgressView> {
    match progress_mode(a).ok()? {
        ProgressMode::Off => None,
        ProgressMode::Json => Some(ProgressView::Json),
        ProgressMode::Auto if a.has("verbose") => None,
        ProgressMode::Auto if host.stderr_is_terminal() => Some(ProgressView::Live { shown: false }),
        ProgressMode::Auto if is_ci(host) => Some(ProgressView::Heartbeat { printed_at: None }),
        ProgressMode::Auto => None,
    }
}

/// Whether the `CI` environment variable says this runs in CI.
pub(in crate::cli) fn is_ci(host: &dyn Host) -> bool {
    host.env_var("CI").is_some_and(|v| !matches!(v.as_str(), "" | "0" | "false"))
}

/// A command's exit status and result, or the status of a failure it has reported.
pub(in crate::cli) type Outcome = Result<(i32, Value), Exit>;

/// Runs a command `body`, which prints text and returns its result. With JSON output the
/// text is held back and the result printed instead, with its `exit_code`; a failure it
/// has reported exits with 2, and prints `{"exit_code", "error"}` with what it reported. `--json-report FILE` gets the
/// JSON in either format.
pub(in crate::cli) fn with_output(
    host: &mut dyn Host,
    a: &Args,
    cmd: &str,
    body: impl FnOnce(&mut dyn Host) -> Outcome,
) -> i32 {
    let format = match output_format(a, host) {
        Ok(f) => f,
        Err(m) => return usage_error(host, &m, Some(cmd)),
    };
    let progress = progress_view(a, host);
    let mut capture = match format {
        Format::Text => Quiet::recording_errors(host),
        Format::Json => Quiet::capturing(host),
    };
    // progress goes straight to stderr, never into the captured errors
    capture.progress = progress;
    let outcome = body(&mut capture);
    let (code, mut result) = outcome.unwrap_or_else(|_| {
        // what the command reported: its errors, else what it printed
        let err = std::mem::take(&mut capture.captured_err);
        let out = capture.captured.take().unwrap_or_else(|| std::mem::take(&mut capture.shown_out));
        let text = if err.trim().is_empty() { out } else { err };
        let text = text.trim();
        (ERROR, json!({"error": text.strip_prefix("error: ").unwrap_or(text)}))
    });
    drop(capture);
    if !result.is_object() {
        result = json!({"result": result});
    }
    result["exit_code"] = json!(code);
    let text = serde_json::to_string_pretty(&result).expect("JSON values serialize") + "\n";
    if format == Format::Json {
        host.write(Stream::Out, &text, None);
    }
    if let Some(path) = a.get("json-report") {
        if let Err(e) = host.write_file(&full_path(&host.cwd(), Path::new(path)), &text) {
            error(host, &format!("cannot write {}: {}", path, e));
            return ERROR;
        }
    }
    code
}

pub(in crate::cli) fn uint(args: &Args, long: &str) -> Result<Option<usize>, String> {
    args.get(long)
        .map(|v| v.parse::<usize>().map_err(|_| format!("'--{}' needs a non-negative whole number, not '{}'", long, v)))
        .transpose()
}

/// A duration in seconds: `90`, `90s`, `5m` or `1h`.
pub(in crate::cli) fn parse_duration(s: &str) -> Option<u32> {
    let (num, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => (&s[..i], &s[i..]),
        None => (s, "s"),
    };
    let n: u32 = num.parse().ok()?;
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return None,
    };
    n.checked_mul(scale).filter(|&t| t > 0)
}

/// `--latency FILE[:PROFILE]`. The profile follows the last `:`, unless that `:` belongs to
/// the path (as in `C:\x.toml`).
pub(in crate::cli) fn split_latency(s: &str) -> (&str, Option<&str>) {
    match s.rsplit_once(':') {
        Some((file, profile))
            if !file.is_empty() && !profile.is_empty() && !profile.contains(['/', '\\']) && file.len() > 1 =>
        {
            (file, Some(profile))
        }
        _ => (s, None),
    }
}

pub(in crate::cli) fn is_source(arg: &str) -> bool {
    let l = arg.to_lowercase();
    l.ends_with(".pproj") || l.ends_with(".p")
}

/// `unknown command 'chek'; did you mean 'check'?`, with the commands.
pub(in crate::cli) fn unknown_command(c: &str) -> String {
    let mut m = format!("unknown command '{}'", c);
    if let Some(s) = crate::syntax::closest(c, COMMANDS) {
        m.push_str(&format!("; did you mean '{}'?", s));
    }
    m.push_str(&format!("{}{}", if m.ends_with('?') { " Commands: " } else { "; commands: " }, COMMANDS.join(", ")));
    m
}

/// Splits positionals into the sources (`.pproj`, `.p`), which it adds to `input`, and the rest.
pub(in crate::cli) fn sources(input: &mut crate::project::Input, positional: &[String]) -> Result<Vec<String>, String> {
    let mut rest = Vec::new();
    for a in positional {
        if a.to_lowercase().ends_with(".pproj") {
            if let Some(p) = &input.pproj {
                return Err(format!("two project files given: '{}' and '{}'; pass one .pproj", p, a));
            }
            input.pproj = Some(a.clone());
        } else if is_source(a) {
            input.pfiles.push(a.clone());
        } else {
            rest.push(a.clone());
        }
    }
    Ok(rest)
}

use crate::checker::observe_inputs::glob_match;

/// The test cases matching `patterns` (all for none), in declaration order.
pub(in crate::cli) fn select_tests(names: &[String], patterns: &[String]) -> Result<Vec<String>, String> {
    if patterns.is_empty() {
        return Ok(names.to_vec());
    }
    for p in patterns {
        if !names.iter().any(|n| glob_match(p, n)) {
            let mut m = format!("no test case matches '{}'. Test cases: {}", p, names.join(", "));
            if let Some(s) = crate::syntax::closest(p, names.iter().map(String::as_str)) {
                m = format!("no test case matches '{}'; did you mean '{}'? Test cases: {}", p, s, names.join(", "));
            }
            if p.contains('/') || p.contains('.') {
                m.push_str(&format!(
                    ". '{}' is read as a test case name; to check files, pass .p or .pproj paths, e.g. `snappea check src/*.p`",
                    p
                ));
            }
            return Err(m);
        }
    }
    Ok(names.iter().filter(|n| patterns.iter().any(|p| glob_match(p, n))).cloned().collect())
}
