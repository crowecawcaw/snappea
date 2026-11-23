//! Port of upstream's `CommandLineArgumentParser`: the same matching rules, value parsing,
//! error messages and help layout, so `p check --help` looks the same.

use std::fmt::Write as _;

/// Value type of an argument (`DataType`); `None` only for positional arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DataType {
    None,
    Str,
    UInt,
    Double,
    Bool,
}

/// A parsed argument value.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Value {
    Str(String),
    UInt(u32),
    Double(f64),
    Bool(bool),
    StrList(Vec<String>),
    UIntList(Vec<u32>),
}

impl Value {
    pub(super) fn as_str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            _ => "",
        }
    }
    pub(super) fn as_uint(&self) -> u32 {
        match self {
            Value::UInt(v) => *v,
            _ => 0,
        }
    }
    pub(super) fn as_str_list(&self) -> Vec<String> {
        match self {
            Value::StrList(v) => v.clone(),
            Value::Str(s) => vec![s.clone()],
            _ => Vec::new(),
        }
    }
    pub(super) fn as_uint_list(&self) -> Vec<u32> {
        match self {
            Value::UIntList(v) => v.clone(),
            Value::UInt(v) => vec![*v],
            _ => Vec::new(),
        }
    }
}

/// One argument definition (`CommandLineArgument`).
#[derive(Debug, Clone)]
pub(super) struct Argument {
    pub long_name: String,
    pub short_name: Option<String>,
    pub data_type: DataType,
    pub description: Option<String>,
    pub required: bool,
    pub positional: bool,
    pub group: Option<String>,
    pub hidden: bool,
    pub multi_value: bool,
    pub allowed_values: Vec<String>,
    pub print_help: bool,
}

/// A parsed argument occurrence.
#[derive(Debug, Clone)]
pub(super) struct Parsed {
    pub long_name: String,
    pub value: Option<Value>,
}

/// Why parsing stopped.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ParseError {
    /// `-h`/`--help`: upstream prints the help and exits with code 1.
    Help,
    /// A `CommandLineException` or argument value error; upstream prints the help followed
    /// by `Error: {message}`.
    Invalid(String),
}

struct Group {
    name: String,
    description: String,
}

/// The parser (`CommandLineArgumentParser`).
pub(super) struct ArgParser {
    app_name: String,
    app_description: String,
    groups: Vec<Group>,
    positional: Vec<String>,
    long_names: Vec<String>,
    /// All arguments in insertion order (the iteration order of upstream's dictionary).
    args: Vec<Argument>,
}

impl ArgParser {
    pub(super) fn new(app_name: &str, app_description: &str) -> ArgParser {
        let mut p = ArgParser {
            app_name: app_name.into(),
            app_description: app_description.into(),
            groups: Vec::new(),
            positional: Vec::new(),
            long_names: Vec::new(),
            args: Vec::new(),
        };
        let h = p.add(None, "help", Some("h"), "Show this help menu", DataType::Bool);
        h.print_help = true;
        p
    }

    pub(super) fn group(&mut self, name: &str, description: &str) {
        if !self.groups.iter().any(|g| g.name == name) {
            self.groups.push(Group { name: name.into(), description: description.into() });
        }
    }

    /// `AddArgument` (`group` = `None` for parser-level arguments).
    pub(super) fn add(
        &mut self,
        group: Option<&str>,
        long: &str,
        short: Option<&str>,
        description: &str,
        data_type: DataType,
    ) -> &mut Argument {
        assert!(!self.args.iter().any(|a| a.long_name == long), "Argument {} already defined", long);
        self.args.push(Argument {
            long_name: long.into(),
            short_name: short.map(String::from),
            data_type,
            description: Some(description.into()),
            required: false,
            positional: false,
            group: group.map(String::from),
            hidden: false,
            multi_value: false,
            allowed_values: Vec::new(),
            print_help: false,
        });
        self.long_names.push(long.into());
        self.args.last_mut().unwrap()
    }

    /// `AddPositionalArgument`.
    pub(super) fn add_positional(&mut self, group: Option<&str>, name: &str, description: &str) -> &mut Argument {
        self.args.push(Argument {
            long_name: name.into(),
            short_name: None,
            data_type: DataType::None,
            description: Some(description.into()),
            required: true,
            positional: true,
            group: group.map(String::from),
            hidden: false,
            multi_value: false,
            allowed_values: Vec::new(),
            print_help: false,
        });
        self.positional.push(name.into());
        self.args.last_mut().unwrap()
    }

    fn get(&self, long: &str) -> &Argument {
        self.args.iter().find(|a| a.long_name == long).unwrap()
    }

    /// `ParseArguments`.
    pub(super) fn parse(&self, args: &[String]) -> Result<Vec<Parsed>, ParseError> {
        let mut result: Vec<Parsed> = Vec::new();
        let mut position = 0usize;
        // index into `result` of the switch currently accepting values
        let mut current: Option<usize> = None;
        for arg in args {
            if let Some(stripped) = arg.strip_prefix('-') {
                let found = if let Some(name) = arg.strip_prefix("--") {
                    self.args.iter().find(|a| a.long_name == name)
                } else {
                    let name = stripped;
                    self.args
                        .iter()
                        .find(|a| a.short_name.as_deref() == Some(name))
                        .or_else(|| self.args.iter().find(|a| a.long_name == name))
                };
                let Some(def) = found else {
                    return Err(ParseError::Invalid(format!("Unexpected argument: '{}'", arg)));
                };
                result.push(Parsed { long_name: def.long_name.clone(), value: None });
                current = Some(result.len() - 1);
                if def.print_help {
                    return Err(ParseError::Help);
                }
            } else if let Some(ci) = current {
                let def = self.get(&result[ci].long_name).clone();
                add_parsed_value(&def, &mut result[ci], arg)?;
            } else {
                let mut def;
                loop {
                    if position < self.positional.len() {
                        def = self.get(&self.positional[position]).clone();
                        position += 1;
                    } else {
                        return Err(ParseError::Invalid(format!("Unexpected positional argument: '{}'", arg)));
                    }
                    if def.required || position >= self.positional.len() {
                        break;
                    }
                }
                let v = parse_value(&def, arg)?;
                result.push(Parsed { long_name: def.long_name.clone(), value: Some(v) });
                current = None;
            }
        }
        for def in &self.args {
            if def.required && !result.iter().any(|r| r.long_name == def.long_name) {
                return Err(ParseError::Invalid(if def.positional {
                    format!("Missing required argument: '{}'", def.long_name)
                } else {
                    format!("Missing required argument: '--{}'", def.long_name)
                }));
            }
        }
        for r in &result {
            let def = self.get(&r.long_name);
            if !def.positional
                && r.value.is_none()
                && def.data_type != DataType::Bool
                && !def.allowed_values.iter().any(|v| v.is_empty())
            {
                return Err(ParseError::Invalid(format!("Missing value for argument: '--{}'", r.long_name)));
            }
        }
        Ok(result)
    }

    /// `PrintHelp`.
    pub(super) fn help(&self) -> String {
        const LINE: usize = 100;
        const INDENT: usize = 30;
        let mut out = String::new();
        let prefix = format!("usage: {} ", self.app_name);
        out.push_str(&prefix);
        let mut w = Wrapper::new(prefix.len(), LINE);
        for name in &self.positional {
            w.word(&mut out, &self.get(name).long_syntax());
        }
        for name in &self.long_names {
            let a = self.get(name);
            if a.hidden {
                continue;
            }
            let mut text = a.long_syntax_and_type();
            if !a.required {
                text = format!("[{}]", text);
            }
            w.word(&mut out, &text);
        }
        out.push('\n');
        out.push('\n');
        let mut w = Wrapper::new(0, LINE);
        w.text(&mut out, &self.app_description);
        out.push('\n');
        out.push('\n');
        let mut visited: Vec<&str> = Vec::new();
        let all: Vec<&String> = self.positional.iter().chain(self.long_names.iter()).collect();
        for g in &self.groups {
            let _ = writeln!(out, "{}:", g.description);
            let _ = writeln!(out, "{}", "-".repeat(g.description.chars().count() + 1));
            for name in &all {
                let a = self.get(name);
                if a.hidden || a.group.as_deref() != Some(g.name.as_str()) {
                    continue;
                }
                visited.push(&a.long_name);
                let mut syntax = String::from("  ");
                if let Some(s) = &a.short_name {
                    let _ = write!(syntax, "-{}, ", s);
                }
                let _ = write!(syntax, "{} ", a.long_syntax_and_type());
                out.push_str(&syntax);
                pad(&mut out, syntax.len(), INDENT);
                if let Some(d) = a.description.as_deref().filter(|d| !d.is_empty()) {
                    out.push_str(": ");
                    let mut w = Wrapper::new(INDENT + 2, LINE);
                    w.text(&mut out, d);
                }
                out.push('\n');
            }
            out.push('\n');
        }
        let mut header = false;
        for name in &all {
            let a = self.get(name);
            if a.hidden || visited.contains(&a.long_name.as_str()) {
                continue;
            }
            if !header {
                header = true;
                let banner = "Optional Arguments:";
                let _ = writeln!(out, "{}", banner);
                let _ = writeln!(out, "{}", "-".repeat(banner.len()));
            }
            let mut syntax = String::from("  ");
            if let Some(s) = &a.short_name {
                let _ = write!(syntax, "-{}, ", s);
            }
            let _ = write!(syntax, "{} ", a.long_syntax());
            out.push_str(&syntax);
            pad(&mut out, syntax.len(), INDENT);
            if let Some(d) = a.description.as_deref().filter(|d| !d.is_empty()) {
                let mut w = Wrapper::new(INDENT, LINE);
                w.text(&mut out, d);
            }
            out.push('\n');
        }
        out
    }
}

fn pad(out: &mut String, len: usize, indent: usize) {
    if len < indent {
        out.push_str(&" ".repeat(indent - len));
    } else {
        out.push('\n');
        out.push_str(&" ".repeat(indent));
    }
}

impl Argument {
    fn long_syntax(&self) -> String {
        if self.positional {
            self.long_name.clone()
        } else {
            format!("--{}", self.long_name)
        }
    }

    fn long_syntax_and_type(&self) -> String {
        let mut text = self.long_syntax();
        if self.data_type != DataType::Bool {
            text.push(' ');
            text.push_str(match self.data_type {
                DataType::Str => "string",
                DataType::UInt => "uint",
                DataType::Double => "double",
                DataType::None | DataType::Bool => "",
            });
        }
        text
    }

    pub(super) fn hidden(&mut self) -> &mut Self {
        self.hidden = true;
        self
    }

    pub(super) fn multi(&mut self) -> &mut Self {
        self.multi_value = true;
        self
    }

    pub(super) fn allowed(&mut self, values: &[&str]) -> &mut Self {
        self.allowed_values = values.iter().map(|s| s.to_string()).collect();
        self
    }

    pub(super) fn optional(&mut self) -> &mut Self {
        self.required = false;
        self
    }
}

/// `ParseValue`.
fn parse_value(def: &Argument, value: &str) -> Result<Value, ParseError> {
    let err = |what: &str| ParseError::Invalid(format!("Argument: '{}' value is not a valid {}", def.long_name, what));
    let v = match def.data_type {
        DataType::None | DataType::Str => Value::Str(value.to_string()),
        DataType::Bool => Value::Bool(parse_bool(value).ok_or_else(|| err("bool"))?),
        DataType::UInt => Value::UInt(parse_int::<u32>(value).ok_or_else(|| err("unsigned integer"))?),
        DataType::Double => Value::Double(value.trim().parse::<f64>().map_err(|_| err("double"))?),
    };
    if !def.allowed_values.is_empty() {
        let s = match &v {
            Value::Str(s) => s.to_lowercase(),
            Value::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Value::UInt(i) => i.to_string(),
            Value::Double(d) => d.to_string(),
            _ => String::new(),
        };
        if !def.allowed_values.contains(&s) {
            if def.hidden {
                return Err(ParseError::Invalid(format!("Argument: '{}' value '{}' is not valid", def.long_name, s)));
            }
            return Err(ParseError::Invalid(format!(
                "Argument: '{}' value '{}' must be one of [{}]",
                def.long_name,
                s,
                def.allowed_values.join(", ")
            )));
        }
    }
    Ok(v)
}

/// .NET `bool.TryParse`: case-insensitive `true`/`false`, surrounding whitespace allowed.
fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// .NET integer `TryParse` with `NumberStyles.Integer`: surrounding whitespace and a sign.
fn parse_int<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim().parse::<T>().ok()
}

/// `AddParsedValue`.
fn add_parsed_value(def: &Argument, p: &mut Parsed, arg: &str) -> Result<(), ParseError> {
    if !def.multi_value {
        if p.value.is_some() {
            return Err(ParseError::Invalid(format!("Argument: '--{}' has too many values", def.long_name)));
        }
        p.value = Some(parse_value(def, arg)?);
        return Ok(());
    }
    let v = parse_value(def, arg)?;
    p.value = Some(match (p.value.take(), v) {
        (None, Value::Str(s)) => Value::StrList(vec![s]),
        (None, Value::UInt(i)) => Value::UIntList(vec![i]),
        (Some(Value::StrList(mut l)), Value::Str(s)) => {
            l.push(s);
            Value::StrList(l)
        }
        (Some(Value::UIntList(mut l)), Value::UInt(i)) => {
            l.push(i);
            Value::UIntList(l)
        }
        (_, v) => v,
    });
    Ok(())
}

/// `WordWrapper`.
struct Wrapper {
    indent: usize,
    line_len: usize,
    current: usize,
}

impl Wrapper {
    fn new(indent: usize, line_len: usize) -> Wrapper {
        Wrapper { indent, line_len, current: indent }
    }

    fn text(&mut self, out: &mut String, text: &str) {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.newline(out);
            }
            for word in line.split(' ') {
                self.word(out, word);
            }
        }
    }

    fn word(&mut self, out: &mut String, word: &str) {
        let n = word.chars().count();
        if self.current + n > self.line_len {
            self.newline(out);
        }
        out.push_str(word);
        out.push(' ');
        self.current += n + 1;
    }

    fn newline(&mut self, out: &mut String) {
        out.push('\n');
        self.current = self.indent;
        if self.indent > 0 {
            out.push_str(&" ".repeat(self.indent));
        }
    }
}
