import { useEffect, useRef, useState } from "react";
import type { CheckResult, Request, WorkerReply } from "../../protocol";
import { encodeShared, MAX_SHARE_CHARS } from "../../share";
import { Code } from "../docs/Code";
import { BROKEN_MODEL, FIXED_MODEL, TEST_SETUP } from "./miniModel";

/** A focused view of the real Playground worker; no checker work until Run. */
export function MiniPlayground() {
  const [model, setModel] = useState(BROKEN_MODEL);
  const [setup, setSetup] = useState(TEST_SETUP);
  const [file, setFile] = useState<"model" | "setup">("model");
  const [editing, setEditing] = useState(false);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CheckResult>();
  const [error, setError] = useState<string>();
  const worker = useRef<Worker | undefined>(undefined);
  const watchdog = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const editor = useRef<HTMLTextAreaElement>(null);
  const source = file === "model" ? model : setup;

  useEffect(() => () => {
    worker.current?.terminate();
    clearTimeout(watchdog.current);
  }, []);
  useEffect(() => { if (editing) editor.current?.focus(); }, [editing, file]);

  function invalidate() {
    worker.current?.terminate();
    worker.current = undefined;
    clearTimeout(watchdog.current);
    setRunning(false);
    setResult(undefined);
    setError(undefined);
  }

  function preset(value: string) {
    invalidate();
    setModel(value);
    setSetup(TEST_SETUP);
    setFile("model");
    setEditing(false);
  }

  function run() {
    invalidate();
    setRunning(true);
    try {
      const w = new Worker(new URL("../../p.worker.ts", import.meta.url), { type: "module" });
      worker.current = w;
      function finish() {
        w.terminate();
        if (worker.current === w) worker.current = undefined;
        clearTimeout(watchdog.current);
        setRunning(false);
      }
      w.onmessage = ({ data }: MessageEvent<WorkerReply>) => {
        if (worker.current !== w || data.progress) return;
        finish();
        if (data.error) setError(data.error);
        else if (data.result?.kind === "check") setResult(data.result);
        else setError("The checker returned an unexpected response. Try running again.");
      };
      w.onerror = () => {
        if (worker.current !== w) return;
        finish();
        setError("The browser could not start the checker. Try again or open the full Playground.");
      };
      watchdog.current = setTimeout(() => {
        finish();
        setError("The checker did not respond. Try again or open the full Playground.");
      }, 30_000);
      const req: Request = {
        kind: "check", files: { "Model.p": model, "Test.p": setup },
        testCase: "tcTwoClients", timeout: 2, seed: 42, strict: false,
        extraArgs: ["--no-mutation"],
      };
      w.postMessage({ id: 1, req });
    } catch {
      worker.current?.terminate();
      worker.current = undefined;
      clearTimeout(watchdog.current);
      setRunning(false);
      setError("The browser could not start the checker. Try the full Playground.");
    }
  }

  const shared = encodeShared({
    files: { "Model.p": model, "Test.p": setup }, open: "Model.p", testCase: "tcTwoClients", timeout: 2, extraArgs: "--no-mutation",
  });
  const shareFits = shared.length <= MAX_SHARE_CHARS;
  const playground = shareFits ? `./playground.html#s=${shared}` : "./playground.html";
  const verdict = result?.exitCode === 0 ? "No bug found" : result?.bug ? "Counterexample found" : "Check needs attention";

  return (
    <div className="mini-playground">
      <div className="mini-toolbar">
        <div className="mini-presets" role="group" aria-label="Counter design">
          <button aria-pressed={model === BROKEN_MODEL && setup === TEST_SETUP} onClick={() => preset(BROKEN_MODEL)}>Read then write</button>
          <button aria-pressed={model === FIXED_MODEL && setup === TEST_SETUP} onClick={() => preset(FIXED_MODEL)}>Atomic increment</button>
        </div>
        <div className="mini-run-controls">
          {result && <span className={result.exitCode === 0 ? "mini-pass" : "mini-error"} role="status">{verdict}</span>}
          {error && <span className="mini-error" role="status">Check could not run</span>}
          <button className="button primary" onClick={run} disabled={running}>{running ? "Checking…" : "Run model"}</button>
          {running && <button onClick={invalidate}>Stop</button>}
        </div>
      </div>
      <div className="mini-workspace">
        <div className="mini-source">
          <div className="mini-filebar">
            <div role="group" aria-label="P source files">
              <button aria-pressed={file === "model"} onClick={() => setFile("model")}>Model.p</button>
              <button aria-pressed={file === "setup"} onClick={() => setFile("setup")}>Test.p</button>
            </div>
            <button onClick={() => setEditing(!editing)}>{editing ? "Done editing" : "Edit P code"}</button>
          </div>
          {editing ? <textarea ref={editor} className="mini-editor" aria-label={`Edit ${file === "model" ? "Model.p" : "Test.p"}`}
            spellCheck={false} value={source} onChange={(e) => {
              invalidate();
              if (file === "model") setModel(e.target.value); else setSetup(e.target.value);
            }} /> : <div className="mini-code" role="region" aria-label={`Read ${file === "model" ? "Model.p" : "Test.p"}`} tabIndex={0}><Code lang="p">{source}</Code></div>}
        </div>
        <div className="mini-result">
          <h3>Two increments should leave 2.</h3>
          <Code lang="p">{`assert final == 2;`}</Code>
          <p>Each client runs independently. The checker chooses which message is delivered next.</p>
          <p><b>Read then write</b> leaves a gap for the other client to run. <b>Atomic increment</b> updates the value in a single handler.</p>
          {setup !== TEST_SETUP && <p className="mini-edited-note">Test.p has been edited. The checker will use your edited assertions, not necessarily the starting property above.</p>}
          <div className="mini-status" role="status" aria-live="polite">
            {error ? <p className="mini-error">{error}</p> : result ? <>
              <h4 className={result.exitCode === 0 ? "mini-pass" : "mini-error"}>{verdict}</h4>
              <p>{result.exitCode === 0 ? "Read the checker output for search completion and warnings. This checks the model within its bounds, not your implementation." : result.bug ? "An execution broke a check. Inspect the output and trace below." : "Read the diagnostics below, edit the code, and run again."}</p>
            </> : <p>{running ? "Loading the checker and exploring event orderings…" : "Run the model, inspect the result, then try the other design or edit the P code."}</p>}
          </div>
        </div>
      </div>
      {result && <div className="mini-output">
        <details open><summary>Checker output</summary><pre>{result.console}</pre></details>
        {result.traceText && <details><summary>Counterexample trace</summary><pre>{result.traceText}</pre></details>}
      </div>}
      <div className="mini-footer">
        <span>2-second exploration budget · mutation checks off for this demo</span>
        <a href={playground}>{shareFits ? "Open these files in the full Playground →" : "Project too large for a link — open Playground and copy your files →"}</a>
      </div>
      <noscript><p>Enable JavaScript to edit and run this model, or follow the <a href="./docs/first-model.html">tutorial</a>.</p></noscript>
    </div>
  );
}
