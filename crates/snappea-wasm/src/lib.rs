//! WebAssembly entry point for the `snappea` tool.
//!
//! The module exposes a tiny C ABI (no wasm-bindgen needed):
//!
//! * `snappea_alloc(len) -> ptr` / `snappea_free(ptr, len)` manage input buffers;
//! * `snappea_run(ptr, len) -> ptr` runs one request and returns a pointer to a result
//!   buffer laid out as a little-endian `u32` length followed by UTF-8 JSON;
//! * `snappea_result_free(ptr)` releases a result buffer.
//!
//! A request is JSON: `{"args": ["check", "m.p", "tcMain"], "files": {"path": "text"},
//! "cwd": "/work", "seed": 42}`. The command runs against an in-memory file system holding
//! `files` and runs [`snappea::cli::run`]; the response carries the exit code, console output and the complete file
//! system afterwards (so a following request can pass it back in, e.g. to replay a
//! schedule). `js/snappea.mjs` wraps this for JavaScript.
//!
//! The host may import `env.snappea_now() -> f64` (seconds) for elapsed-time reporting;
//! when targeting WebAssembly it is required.
//!
//! With `"progress": true` in the request, a running `snappea check` calls the import
//! `env.snappea_progress(ptr, len)` a few times a second with a progress snapshot as
//! UTF-8 JSON (see `snappea::driver::progress::Progress::to_json`), e.g.
//! `{"phase":"check","test_case":"tcMain","schedules":1240,"elapsed":3.1,"time_limit":10,...}`.
//! The buffer is only valid during the call. When targeting WebAssembly the import is
//! required; it may do nothing.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use snappea::host::{Host, MemHost};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub args: Vec<String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub seed: Option<u32>,
    /// Send progress snapshots to `env.snappea_progress`.
    #[serde(default)]
    pub progress: bool,
}

#[derive(Debug, Serialize)]
pub struct Response {
    #[serde(rename = "exitCode")]
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub files: BTreeMap<String, String>,
}

/// The embedder's clock.
#[cfg(target_arch = "wasm32")]
fn now() -> f64 {
    #[link(wasm_import_module = "env")]
    extern "C" {
        fn snappea_now() -> f64;
    }
    unsafe { snappea_now() }
}

/// Passes a progress snapshot to the embedder.
#[cfg(target_arch = "wasm32")]
fn progress(p: &snappea::driver::progress::Progress) {
    #[link(wasm_import_module = "env")]
    extern "C" {
        fn snappea_progress(ptr: *const u8, len: usize);
    }
    let json = p.to_json().to_string();
    unsafe { snappea_progress(json.as_ptr(), json.len()) }
}

/// Runs one request (usable natively too, e.g. for tests).
pub fn run(req: Request) -> Response {
    let cwd = req.cwd.unwrap_or_else(|| "/work".into());
    let mut mem = MemHost::new(&cwd);
    mem.seed = req.seed.unwrap_or(0);
    #[cfg(target_arch = "wasm32")]
    {
        mem.clock = now;
        if req.progress {
            mem.on_progress = Some(progress);
        }
    }
    for (path, text) in &req.files {
        if let Err(e) = mem.write_file(Path::new(path), text) {
            return invalid_request(&format!("cannot add file {}: {}", path, e));
        }
    }
    let exit_code = snappea::cli::run(&req.args, &mut mem);
    let mut files = BTreeMap::new();
    for p in mem.files_under("/") {
        if let Ok(t) = mem.read_to_string(&p) {
            files.insert(p.to_string_lossy().to_string(), t);
        }
    }
    Response { exit_code, stdout: mem.stdout, stderr: mem.stderr, files }
}

/// The response to a request that cannot be run: exit code 2 and the reason on stderr.
fn invalid_request(reason: &str) -> Response {
    Response {
        exit_code: 2,
        stdout: String::new(),
        stderr: format!("invalid request: {}\n", reason),
        files: BTreeMap::new(),
    }
}

/// Runs a JSON request and returns the JSON response.
pub fn run_json(input: &str) -> String {
    let resp = match serde_json::from_str::<Request>(input) {
        Ok(req) => run(req),
        Err(e) => invalid_request(&e.to_string()),
    };
    serde_json::to_string(&resp).expect("serializable")
}

#[no_mangle]
pub extern "C" fn snappea_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` must come from [`snappea_alloc`].
#[no_mangle]
pub unsafe extern "C" fn snappea_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// # Safety
/// `ptr..ptr+len` must be an allocated buffer holding UTF-8 JSON.
#[no_mangle]
pub unsafe extern "C" fn snappea_run(ptr: *const u8, len: usize) -> *mut u8 {
    let input = std::slice::from_raw_parts(ptr, len);
    let out = run_json(&String::from_utf8_lossy(input));
    let bytes = out.into_bytes();
    let mut buf = Vec::with_capacity(bytes.len() + 4);
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&bytes);
    let mut buf = buf.into_boxed_slice();
    let p = buf.as_mut_ptr();
    std::mem::forget(buf);
    p
}

/// # Safety
/// `ptr` must come from [`snappea_run`].
#[no_mangle]
pub unsafe extern "C" fn snappea_result_free(ptr: *mut u8) {
    let len = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize + 4;
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_round_trip() {
        let mut files = BTreeMap::new();
        files.insert(
            "/work/m.p".to_string(),
            "// snappea-lint: allow no-model-machine -- Wasm round-trip fixture checks failure output from the entry machine\nmachine Main { start state S { entry { assert false, \"no\"; } } }".to_string(),
        );
        let r =
            run(Request { args: vec!["check".into(), "m.p".into()], files, cwd: None, seed: Some(7), progress: false });
        assert_eq!(r.exit_code, 1, "{}", r.stdout);
        assert!(r.stdout.contains("\nFAIL  "), "{}", r.stdout);
        assert!(r.stdout.contains("Replay: snappea replay PCheckerOutput/"), "{}", r.stdout);
        let trace = r.files.iter().find(|(k, _)| k.ends_with("/bug.txt")).map(|(_, v)| v.clone()).unwrap();
        assert!(trace.contains("<ErrorLog> Assertion Failed: m.p:2:40 no"), "{}", trace);
        assert!(r.files.keys().any(|k| k.ends_with(".trace.json")));
    }

    #[test]
    fn conflicting_files_are_an_invalid_request() {
        let files =
            BTreeMap::from([("/work/d".to_string(), String::new()), ("/work/d/f.p".to_string(), String::new())]);
        let r = run(Request { args: vec!["check".into()], files, cwd: None, seed: None, progress: false });
        assert_eq!(r.exit_code, 2);
        assert!(r.stderr.starts_with("invalid request: cannot add file /work/d/f.p: "), "{}", r.stderr);
    }
}
