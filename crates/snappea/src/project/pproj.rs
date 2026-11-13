//! `.pproj` project files (`ParsePProjectFile`).

use std::path::{Path, PathBuf};

use crate::host::{full_path, is_hidden, report_and_exit_hint, Exit, Host, HostExt};

/// What a project file contributes: its name and sources. `OutputDir` and `Target` only
/// matter to upstream's compiler and are ignored.
#[derive(Debug, Clone, Default)]
pub struct Project {
    pub name: String,
    /// Input files (P and foreign), absolute, in discovery order.
    pub input_files: Vec<PathBuf>,
    pub project_root: PathBuf,
    /// `<pobserve-package>`: Java package of the PObserve backend's generated classes.
    pub pobserve_package: Option<String>,
}

/// The elements of a project file snappea reads.
struct Doc {
    path: PathBuf,
    /// `<ProjectName>` texts.
    project_names: Vec<String>,
    /// `<InputFiles><PFile>` texts.
    input_files: Vec<String>,
    /// `<IncludeProject>` texts.
    include_projects: Vec<String>,
    pobserve_package: Option<String>,
}

impl Doc {
    fn load(host: &dyn Host, path: &Path) -> Result<Doc, String> {
        let text = host.read_to_string(path).map_err(|e| e.to_string())?;
        let xml = roxmltree::Document::parse(&text).map_err(|e| format!("malformed XML: {}", e))?;
        let root = xml.root_element();
        let children = |name: &'static str| root.children().filter(move |c| c.has_tag_name(name));
        let texts = |name| children(name).map(|c| element_value(&c)).collect::<Vec<_>>();
        Ok(Doc {
            path: path.to_path_buf(),
            project_names: texts("ProjectName"),
            input_files: children("InputFiles")
                .flat_map(|p| p.children().filter(|c| c.has_tag_name("PFile")).map(|c| element_value(&c)))
                .collect(),
            include_projects: texts("IncludeProject"),
            pobserve_package: texts("pobserve-package").into_iter().next(),
        })
    }
}

/// `XElement.Value`: the concatenated text of all descendants.
fn element_value(n: &roxmltree::Node) -> String {
    n.descendants().filter(|d| d.is_text()).map(|d| d.text().unwrap_or("")).collect()
}

fn is_p_file(p: &str) -> bool {
    p.to_lowercase().ends_with(".p")
}

fn is_foreign_file(p: &str) -> bool {
    let l = p.to_lowercase();
    l.ends_with(".cs") || l.ends_with(".java")
}

/// `CheckFileValidity.IsLegalProjectName`.
pub fn is_legal_project_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `CheckFileValidity.IsLegalPProjFile`: returns the full path when legal.
pub fn legal_pproj(host: &dyn Host, file: &str) -> (bool, PathBuf) {
    let full = full_path(&host.cwd(), Path::new(file));
    (file.len() > 2 && file.ends_with(".pproj") && host.is_file(&full), full)
}

/// `CheckFileValidity.IsLegalPFile`.
pub fn legal_p_file(host: &dyn Host, file: &str) -> Option<PathBuf> {
    let full = full_path(&host.cwd(), Path::new(file));
    (file.len() > 2 && is_p_file(file) && host.is_file(&full)).then_some(full)
}

fn legal_foreign_file(host: &dyn Host, file: &str) -> Option<PathBuf> {
    let full = full_path(&host.cwd(), Path::new(file));
    (file.len() > 2 && is_foreign_file(file) && host.is_file(&full)).then_some(full)
}

/// Recursive file listing in .NET `Directory.GetFiles(..., RecurseSubdirectories)` order:
/// a directory's files first, then its subdirectories breadth-first, at most `max_depth`
/// levels below `dir`.
pub fn list_files(host: &dyn Host, dir: &Path, max_depth: Option<usize>, out: &mut Vec<PathBuf>) {
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((dir.to_path_buf(), 0usize));
    while let Some((d, depth)) = queue.pop_front() {
        let Ok(entries) = host.read_dir(&d) else { continue };
        for e in &entries {
            if host.is_file(e) {
                out.push(e.clone());
            }
        }
        if max_depth.map(|m| depth < m).unwrap_or(true) {
            for e in entries {
                if host.is_dir(&e) {
                    queue.push_back((e, depth + 1));
                }
            }
        }
    }
}

struct ParseErr(String);

/// Where `<IncludeProject>` paths are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Includes {
    /// Against the current folder, as upstream does.
    #[default]
    FromCurrentFolder,
    /// Against the folder of the project file that includes them, falling back to the
    /// current folder.
    FromProject,
}

/// `ParseProjectFileForCompiler`.
pub fn parse(host: &mut dyn Host, project_file: &str, includes: Includes) -> Result<Project, Exit> {
    match parse_inner(host, project_file, includes) {
        Ok(p) => Ok(p),
        Err(Err2::Parse(m)) => {
            let hint = if m.starts_with("Illegal P project file name") {
                "check the path; a project file's name ends in .pproj"
            } else {
                "a .pproj looks like <Project><ProjectName>Name</ProjectName><InputFiles><PFile>./PSrc/</PFile></InputFiles></Project>"
            };
            Err(report_and_exit_hint(host, &format!("<Error parsing project file>:\n {}", m), hint))
        }
        Err(Err2::Exit(e)) => Err(e),
    }
}

enum Err2 {
    Parse(String),
    Exit(Exit),
}

impl From<ParseErr> for Err2 {
    fn from(e: ParseErr) -> Self {
        Err2::Parse(e.0)
    }
}

fn parse_inner(host: &mut dyn Host, project_file: &str, includes: Includes) -> Result<Project, Err2> {
    let (ok, full) = legal_pproj(host, project_file);
    if !ok {
        return Err(Err2::Parse(format!(
            "Illegal P project file name {} or file {} not found",
            project_file,
            full.display()
        )));
    }
    host.out_line("----------------------------------------");
    host.out_line(&format!("==== Loading project file: {}", project_file));
    let doc = Doc::load(host, &full).map_err(ParseErr)?;
    let mut inputs = Vec::new();
    all_dependencies(host, &doc, includes, &mut inputs, &mut Vec::new())?;
    if inputs.is_empty() {
        return Err(Err2::Exit(report_and_exit_hint(
            host,
            "At least one .p file must be provided as input files, no input files found after parsing the project file",
            "list the sources under <InputFiles>, e.g. <PFile>./PSrc/</PFile>, and check that the folders hold .p files",
        )));
    }
    let Some(name) = doc.project_names.first().cloned() else {
        return Err(Err2::Parse(format!("Missing project name in {}", full.display())));
    };
    if !is_legal_project_name(&name) {
        return Err(Err2::Parse(format!("{} is not a legal project name", name)));
    }
    host.out_line("----------------------------------------");
    Ok(Project {
        name,
        input_files: inputs,
        project_root: full.parent().map(Path::to_path_buf).unwrap_or_default(),
        pobserve_package: doc.pobserve_package,
    })
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}

/// `GetAllProjectDependencies`: collects the input files of `doc` and the projects it
/// includes; native imports identify projects by file, while compatibility mode
/// preserves upstream's directory-based deduplication.
fn all_dependencies(
    host: &mut dyn Host,
    doc: &Doc,
    includes: Includes,
    inputs: &mut Vec<PathBuf>,
    deps: &mut Vec<PathBuf>,
) -> Result<(), Err2> {
    for f in read_all_input_files(host, doc) {
        push_unique(inputs, f);
    }
    for dep in &doc.include_projects {
        // upstream resolves included projects against the current directory
        let (mut ok, mut full) = legal_pproj(host, dep);
        if includes == Includes::FromProject {
            let base = doc.path.parent().map(Path::to_path_buf).unwrap_or_default();
            let beside = base.join(dep).to_string_lossy().to_string();
            if let (true, f) = legal_pproj(host, &beside) {
                (ok, full) = (true, f);
            }
        }
        if !ok {
            return Err(Err2::Parse(format!(
                "Illegal P project file name {} or file {} not found",
                dep,
                full.display()
            )));
        }
        host.out_line(&format!("==== Loading project file: {}", full.display()));
        let key = match includes {
            Includes::FromProject => full.clone(),
            Includes::FromCurrentFolder => full.parent().map(Path::to_path_buf).unwrap_or_default(),
        };
        if deps.contains(&key) {
            continue;
        }
        deps.push(key);
        let dep_doc = Doc::load(host, &full).map_err(ParseErr)?;
        all_dependencies(host, &dep_doc, includes, inputs, deps)?;
    }
    Ok(())
}

/// `ReadAllInputFiles`.
fn read_all_input_files(host: &mut dyn Host, doc: &Doc) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let base = doc.path.parent().map(Path::to_path_buf).unwrap_or_default();
    for entry in &doc.input_files {
        let combined = base.join(entry);
        let mut candidates = Vec::new();
        if host.is_dir(&combined) {
            let mut files = Vec::new();
            list_files(host, &combined, None, &mut files);
            for f in files {
                let s = f.to_string_lossy().to_string();
                if (is_p_file(&s) || is_foreign_file(&s)) && !is_hidden(&f) {
                    candidates.push(s);
                }
            }
        } else {
            candidates.push(combined.to_string_lossy().to_string());
        }
        for c in candidates {
            if let Some(full) = legal_p_file(host, &c) {
                host.out_line(&format!("....... includes p file: {}", full.display()));
                push_unique(&mut out, full);
            } else if let Some(full) = legal_foreign_file(host, &c) {
                host.out_line(&format!("....... includes foreign file: {}", full.display()));
                push_unique(&mut out, full);
            }
        }
    }
    out
}

/// `ProjectFileLocator.FindLocalPProject`: the first non-hidden `*.pproj` in the current
/// directory, or `None`.
pub fn find_local_pproj(host: &mut dyn Host) -> Option<String> {
    host.out_line(".. Searching for a P project file *.pproj locally in the current folder");
    let cwd = host.cwd();
    let found = host
        .read_dir(&cwd)
        .unwrap_or_default()
        .into_iter()
        .find(|p| host.is_file(p) && !is_hidden(p) && p.extension().map(|e| e == "pproj").unwrap_or(false));
    match found {
        None => {
            host.out_line(&format!(".. No P project file *.pproj found in the current folder: {}", cwd.display()));
            None
        }
        Some(p) => {
            let s = p.to_string_lossy().to_string();
            host.out_line(&format!(".. Found P project file: {}", s));
            Some(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MemHost;

    #[test]
    fn native_imports_distinguish_sibling_projects_and_stop_cycles() {
        let mut host = MemHost::new("/w");
        host.add_file("App.pproj", "<Project><ProjectName>App</ProjectName><InputFiles><PFile>Main.p</PFile></InputFiles><IncludeProject>lib/A.pproj</IncludeProject><IncludeProject>lib/../lib/B.pproj</IncludeProject></Project>");
        host.add_file("lib/A.pproj", "<Project><ProjectName>A</ProjectName><InputFiles><PFile>A.p</PFile></InputFiles><IncludeProject>lib/B.pproj</IncludeProject></Project>");
        host.add_file("lib/B.pproj", "<Project><ProjectName>B</ProjectName><InputFiles><PFile>B.p</PFile></InputFiles><IncludeProject>lib/A.pproj</IncludeProject></Project>");
        for file in ["Main.p", "lib/A.p", "lib/B.p"] {
            host.add_file(file, "");
        }
        let native = parse(&mut host, "App.pproj", Includes::FromProject).unwrap();
        assert_eq!(native.input_files, ["/w/Main.p", "/w/lib/A.p", "/w/lib/B.p"].map(PathBuf::from));
        // Upstream identifies included dependencies by directory; preserve that behavior.
        let compat = parse(&mut host, "App.pproj", Includes::FromCurrentFolder).unwrap();
        assert_eq!(compat.input_files, ["/w/Main.p", "/w/lib/A.p"].map(PathBuf::from));
    }
}
