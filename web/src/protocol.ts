// Messages between the UI and the worker that runs the WebAssembly `p` tool.
import type { Progress } from "../../crates/snappea-wasm/js/snappea.mjs";

export type { Progress };

export type Files = Record<string, string>;

export interface CompileRequest {
  kind: "compile";
  files: Files;
}

export interface CheckRequest {
  kind: "check";
  files: Files;
  testCase: string;
  timeout: number;
  seed: number;
  /** Fail on lint findings and coverage gaps, as the CLI does; otherwise report them as warnings. */
  strict: boolean;
  extraArgs: string[];
}

export type Request = CompileRequest | CheckRequest;

export interface Diagnostic {
  file: string;
  line: number;
  col: number;
  message: string;
}

export interface CompileResult {
  kind: "compile";
  ok: boolean;
  console: string;
  diagnostics: Diagnostic[];
  tests: string[];
  millis: number;
}

export interface CheckResult {
  kind: "check";
  ok: boolean; // compiled and ran (bug or not)
  bug: boolean;
  exitCode: number;
  console: string;
  diagnostics: Diagnostic[];
  testCase: string;
  /** The buggy schedule's JSON trace (`bug.trace.json`). */
  traceJson?: string;
  /** The buggy schedule's text log (`bug.txt`). */
  traceText?: string;
  /** Replayable schedule file. */
  schedule?: string;
  millis: number;
}

export type Result = CompileResult | CheckResult;

export interface WorkerReply {
  id: number;
  result?: Result;
  error?: string;
  /** A snapshot of the running check; more replies follow. */
  progress?: Progress;
}
