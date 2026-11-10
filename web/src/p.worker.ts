/// <reference lib="webworker" />
// Runs the WebAssembly build of the `snappea` tool. Each request starts from the user's
// files in a fresh in-memory file system rooted at /work.
import { loadP, type PTool, type PResult } from "../../crates/snappea-wasm/js/snappea.mjs";
import wasmUrl from "./generated/snappea.wasm?url";
import type { CheckResult, CompileResult, Diagnostic, Files, Request, RunResult, WorkerReply } from "./protocol";

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

const ERROR = /^error(?:\[[\w-]+\])?:\s*(.*)/;
const ARROW = /^\s*--> ([^\s:]+\.p):(\d+):(\d+)/;
const HELP = /^\s*= (help|hint): (.*)/;

/** Reads `snappea` diagnostics (`error: ...` then `--> a.p:3:5`) and the upstream `[a.p:3:5] ...` form. */
function parseDiagnostics(text: string): Diagnostic[] {
  const out: Diagnostic[] = [];
  let message: string | undefined;
  for (const line of text.split("\n")) {
    const e = ERROR.exec(line);
    if (e) {
      message = e[1];
      continue;
    }
    const at = message !== undefined ? ARROW.exec(line) : null;
    if (at) {
      out.push({ file: at[1].replace(`${ROOT}/`, ""), line: Number(at[2]), col: Number(at[3]), message: message! });
      message = undefined;
      continue;
    }
    const help = HELP.exec(line);
    if (help && out.length && !out[out.length - 1].message.includes("\n")) {
      out[out.length - 1].message += `\n${help[1]}: ${help[2]}`;
      continue;
    }
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

/** Checker output (PCheckerOutput/...) from the last run, so `snappea replay` can use it. */
let outputs: Files = {};

function keepOutputs(files: Files) {
  outputs = Object.fromEntries(Object.entries(files).filter(([k]) => k.startsWith(`${ROOT}/PCheckerOutput/`)));
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

async function handle(id: number, req: Request): Promise<CompileResult | CheckResult | RunResult> {
  const t0 = performance.now();
  if (req.kind === "run") {
    const r = (await p()).run(req.args, { ...outputs, ...toFs(req.files) }, {
      seed: Math.floor(Math.random() * 1_000_000),
      onProgress: (progress) => post({ id, progress }),
    });
    keepOutputs(r.files);
    return { kind: "run", exitCode: r.exitCode, console: (r.stdout + r.stderr).trimEnd() };
  }
  const c = await compile(req.files);
  if (req.kind === "compile") {
    return {
      kind: "compile",
      ok: c.out.exitCode === 0,
      console: c.console,
      diagnostics: c.diagnostics,
      tests: c.tests,
      model: c.out.exitCode === 0
        ? JSON.parse((await p()).run(["playground-model"], toFs(req.files)).stdout)
        : undefined,
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
  // people exploring a model see lint findings and coverage gaps as warnings unless they
  // ask for the CLI's strict defaults; a `--warn`/`--deny` in the extra options wins
  const gates = req.strict || req.extraArgs.some((a, i) => /^--(warn|deny)(=all)?$/.test(a) && (a.endsWith("=all") || req.extraArgs[i + 1] === "all"))
    ? []
    : ["--warn", "all"];
  const opts = ["--inspect-trace", "--timeout", `${req.timeout}s`, ...mutation, "--seed", String(req.seed), ...gates, ...req.extraArgs];
  const shown = ["check", req.testCase, ...opts];
  const args = ["check", ...sourceArgs(req.files), req.testCase, ...opts];
  const r = (await p()).run(args, toFs(req.files), {
    seed: req.seed,
    // delivered while the check runs: only this worker is busy
    onProgress: (progress) => post({ id, progress }),
  });
  keepOutputs(r.files);
  const dir = `${ROOT}/PCheckerOutput/${req.testCase}/`;
  const file = (name: string): string | undefined => r.files[dir + name];
  const schedules = file("schedules.json") ? JSON.parse(file("schedules.json")!) : undefined;
  const console = `$ snappea ${shown.join(" ")}\n` + (r.stdout + r.stderr).trimEnd();
  return {
    kind: "check",
    ok: true,
    bug: r.exitCode === 1,
    exitCode: r.exitCode,
    console,
    diagnostics: parseDiagnostics(console),
    testCase: req.testCase,
    traceJson: file("bug.trace.json") ?? schedules?.samples[0]?.trace,
    schedules,
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
