/// <reference lib="webworker" />
// Runs the WebAssembly build of the `snappea` tool. Each request starts from the user's
// files in a fresh in-memory file system rooted at /work.
import { loadP, type PTool, type PResult } from "../../crates/snappea-wasm/js/snappea.mjs";
import wasmUrl from "./generated/snappea.wasm?url";
import type { CheckResult, CompileResult, Diagnostic, Files, Request, WorkerReply } from "./protocol";

const ROOT = "/work";
let tool: Promise<PTool> | undefined;

function p(): Promise<PTool> {
  tool ??= loadP(fetch(wasmUrl));
  return tool;
}

function toFs(files: Files): Files {
  const out: Files = {};
  for (const [k, v] of Object.entries(files)) out[`${ROOT}/${k}`] = v;
  return out;
}

function sourceArgs(files: Files): string[] {
  return Object.keys(files).filter((f) => f.endsWith(".p")).sort();
}

const ERR_LOC = /\[([^\]\s]+\.p):(\d+):(\d+)\]\s*(.*)/;
const PARSE_ERR = /\[([^\]\s]+\.p)\]\s*parse error: line (\d+):(\d+)\s*(.*)/;

function parseDiagnostics(text: string): Diagnostic[] {
  const out: Diagnostic[] = [];
  for (const line of text.split("\n")) {
    const m = PARSE_ERR.exec(line) ?? ERR_LOC.exec(line);
    if (!m) continue;
    const parse = m.length === 5 && line.includes("parse error");
    out.push({
      file: m[1].replace(`${ROOT}/`, ""),
      line: Number(m[2]),
      // Parser columns are 0-based, checker columns 1-based.
      col: parse ? Number(m[3]) + 1 : Number(m[3]),
      message: m[4],
    });
  }
  return out;
}

let cache: { key: string; out: PResult; tests: string[]; console: string; diagnostics: Diagnostic[] } | undefined;

async function compile(files: Files) {
  const key = JSON.stringify(files);
  if (cache?.key === key) return cache;
  const t = await p();
  // Type-checks the sources and lists the test cases.
  const out = t.run(["tests", ...sourceArgs(files)], toFs(files), { seed: 0 });
  const console = out.exitCode === 0 ? "" : (out.stdout + out.stderr).trimEnd();
  const tests = out.exitCode === 0 ? out.stdout.split("\n").filter((l) => l.trim()) : [];
  cache = { key, out, tests, console, diagnostics: parseDiagnostics(console) };
  return cache;
}

async function handle(id: number, req: Request): Promise<CompileResult | CheckResult> {
  const t0 = performance.now();
  const c = await compile(req.files);
  if (req.kind === "compile") {
    return {
      kind: "compile",
      ok: c.out.exitCode === 0,
      console: c.console,
      diagnostics: c.diagnostics,
      tests: c.tests,
      millis: performance.now() - t0,
    };
  }
  if (c.out.exitCode !== 0) {
    return {
      kind: "check", ok: false, bug: false, exitCode: c.out.exitCode, console: c.console,
      diagnostics: c.diagnostics, testCase: req.testCase, millis: performance.now() - t0,
    };
  }
  // the browser checks on one thread: mutation testing gets the test case's time limit
  // unless the extra options set it
  const mutation = req.extraArgs.some((a) => /^--(no-mutation|mutation-timeout)\b/.test(a))
    ? []
    : ["--mutation-timeout", `${req.timeout}s`];
  const opts = ["--timeout", `${req.timeout}s`, ...mutation, "--seed", String(req.seed), ...req.extraArgs];
  const shown = ["check", req.testCase, ...opts];
  const args = ["check", ...sourceArgs(req.files), req.testCase, ...opts];
  const r = (await p()).run(args, toFs(req.files), {
    seed: req.seed,
    // delivered while the check runs: only this worker is busy
    onProgress: (progress) => post({ id, progress }),
  });
  const dir = `${ROOT}/PCheckerOutput/${req.testCase}/`;
  const file = (name: string): string | undefined => r.files[dir + name];
  const console = `$ snappea ${shown.join(" ")}\n` + (r.stdout + r.stderr).trimEnd();
  return {
    kind: "check",
    ok: true,
    bug: r.exitCode === 1,
    exitCode: r.exitCode,
    console,
    diagnostics: parseDiagnostics(console),
    testCase: req.testCase,
    traceJson: file("bug.trace.json"),
    traceText: file("bug.txt"),
    schedule: file("bug.schedule"),
    millis: performance.now() - t0,
  };
}

self.onmessage = async (e: MessageEvent<{ id: number; req: Request }>) => {
  const { id, req } = e.data;
  let reply: WorkerReply;
  try {
    reply = { id, result: await handle(id, req) };
  } catch (err) {
    // A trap (e.g. a Rust panic) poisons the instance; start over next time.
    tool = undefined;
    cache = undefined;
    reply = { id, error: String(err) };
  }
  post(reply);
};

function post(reply: WorkerReply) {
  (self as unknown as DedicatedWorkerGlobalScope).postMessage(reply);
}
