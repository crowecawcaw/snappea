import { useEffect, useRef, useState } from "react";
import type { CheckResult, Request, WorkerReply } from "../../protocol";
import { encodeShared, MAX_SHARE_CHARS } from "../../share";
import { Code } from "../docs/Code";
import { BROKEN_FOCUS, BROKEN_MODEL, FIXED_FOCUS, FIXED_HANDLER, FIXED_MODEL, TEST_SETUP } from "./miniModel";

/** The counter example: a few lines and the known result first, the real Playground worker on Run. */
export function MiniPlayground() {
  const [model, setModel] = useState(BROKEN_MODEL);
  const [setup, setSetup] = useState(TEST_SETUP);
  const [file, setFile] = useState<"model" | "setup">("model");
  const [editing, setEditing] = useState(false);
  const [full, setFull] = useState(false);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CheckResult>();
  const [error, setError] = useState<string>();
  const worker = useRef<Worker | undefined>(undefined);
  const watchdog = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const editor = useRef<HTMLTextAreaElement>(null);
  const source = file === "model" ? model : setup;
  // Which starting design is on screen, unedited; the written-out result only applies to these.
  const design = setup !== TEST_SETUP ? undefined : model === BROKEN_MODEL ? "broken" : model === FIXED_MODEL ? "fixed" : undefined;
  const showFull = full || !design;

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
    setFull(false);
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
          {!showFull ? <div className="mini-focus">
            <p className="mini-label">{design === "broken" ? "Each client reads, then writes" : "Each client sends one message"} <span>Model.p</span></p>
            <Code lang="p">{design === "broken" ? BROKEN_FOCUS : FIXED_FOCUS}</Code>
            {design === "fixed" && <>
              <p className="mini-label">The counter adds one in a single step <span>Model.p</span></p>
              <Code lang="p">{FIXED_HANDLER}</Code>
            </>}
            <p className="mini-label">The test starts two clients, then checks the total <span>Test.p</span></p>
            <Code lang="p">{`assert final == 2;`}</Code>
            <button onClick={() => setFull(true)}>Show the full model</button>
          </div> : <><div className="mini-filebar">
            <div role="group" aria-label="P source files">
              <button aria-pressed={file === "model"} onClick={() => setFile("model")}>Model.p</button>
              <button aria-pressed={file === "setup"} onClick={() => setFile("setup")}>Test.p</button>
            </div>
            <div>
              {design && !editing && <button onClick={() => setFull(false)}>Show less</button>}
              <button onClick={() => setEditing(!editing)}>{editing ? "Done editing" : "Edit P code"}</button>
            </div>
          </div>
          {editing ? <textarea ref={editor} className="mini-editor" aria-label={`Edit ${file === "model" ? "Model.p" : "Test.p"}`}
            spellCheck={false} value={source} onChange={(e) => {
              invalidate();
              if (file === "model") setModel(e.target.value); else setSetup(e.target.value);
            }} /> : <div className="mini-code" role="region" aria-label={`Read ${file === "model" ? "Model.p" : "Test.p"}`} tabIndex={0}><Code lang="p">{source}</Code></div>}</>}
        </div>
        <div className="mini-result">
          {setup !== TEST_SETUP && <p className="mini-edited-note">Test.p has been edited. The checker will use your edited assertions.</p>}
          <div className="mini-status" role="status" aria-live="polite">
            {error ? <p className="mini-error">{error}</p> : running ? <p>Loading the checker and trying message orderings…</p> : result ? <>
              <h3 className={result.exitCode === 0 ? "mini-pass" : "mini-error"}>{verdict}</h3>
              <p>{result.exitCode === 0 ? "This checks the model within its bounds, not your implementation. Read the output below for warnings." : result.bug ? "One ordering broke a check. The output and trace are below." : "Read the diagnostics below, edit the code, and run again."}</p>
            </> : design === "broken" ? <>
              <h3>The checker finds an ordering that loses an increment</h3>
              <ol className="mini-steps">
                <li>Client A reads <b>0</b></li>
                <li>Client B reads <b>0</b></li>
                <li>Client A writes <b>1</b></li>
                <li>Client B writes <b>1</b></li>
              </ol>
              <p className="mini-fail">Assertion failed: expected 2 increments, counter is 1</p>
              <p className="mini-note">This is the result for the model as written. Run it to get the checker's own output, then try <b>Atomic increment</b>.</p>
            </> : design === "fixed" ? <>
              <h3>The checker tries every ordering and none fails</h3>
              <p>The counter handles one message at a time, so nothing can arrive between reading the value and writing it.</p>
              <p className="mini-note">This is the result for the model as written. Run it to get the checker's own output.</p>
            </> : <p>Run the model to check your edit.</p>}
          </div>
        </div>
      </div>
      {result && <div className="mini-output">
        <details open><summary>Checker output</summary><pre>{result.console}</pre></details>
        {result.traceText && <details><summary>Counterexample trace</summary><pre>{result.traceText}</pre></details>}
      </div>}
      <div className="mini-footer">
        <span>Runs in your browser; no code leaves this page. 2-second budget, mutation testing off.</span>
        <a href={playground}>{shareFits ? "Open these files in the full Playground →" : "Project too large for a link — open Playground and copy your files →"}</a>
      </div>
      <noscript><p>Enable JavaScript to edit and run this model, or follow the <a href="./docs/first-model.html">tutorial</a>.</p></noscript>
    </div>
  );
}
