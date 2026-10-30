// Types for snappea.mjs.

export interface PResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  /** The whole in-memory file system after the command ({absolutePath: text}). */
  files: Record<string, string>;
}

/** A snapshot of a running check (`snappea check --progress json` prints the same). */
export type Progress = {
  elapsed: number;
  /** Seconds, if the phase has a time limit. */
  time_limit: number | null;
  /** The last snapshot of the phase. */
  done: boolean;
} & (
  | {
      phase: "check";
      test_case: string;
      /** One-based position of the test case, of `tests`. */
      test: number;
      tests: number;
      schedules: number;
      schedule_limit: number | null;
      bugs: number;
      workers: number;
    }
  | { phase: "mutation"; detected: number; mutants: number }
);

export interface RunOptions {
  cwd?: string;
  seed?: number;
  /** Called a few times a second while a check runs. */
  onProgress?: (p: Progress) => void;
}

export interface PTool {
  /** Runs `p <args>` over an in-memory file system ({absolutePath: text}). */
  run(args: string[], files?: Record<string, string>, opts?: RunOptions): PResult;
}

export function loadP(source: BufferSource | Response | PromiseLike<Response>): Promise<PTool>;
