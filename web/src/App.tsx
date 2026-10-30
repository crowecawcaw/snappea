import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CUSTOM, DEMOS, loadDemo, sortPaths, type Demo } from "./demos";
import { filesFromDrop, filesFromList } from "./importFiles";
import { decodeShared, sharedFromHash, urlFor, MAX_SHARE_CHARS, type Shared } from "./share";
import { PEditor } from "./editor/PEditor";
import type { CheckResult, Diagnostic, Files, Progress } from "./protocol";
import { Runner } from "./runner";
import { TraceView } from "./trace/TraceView";
import { Logo } from "./site/Logo";

/** Output tabs. "files" is the editor, which is a tab only on phones. */
type Tab = "files" | "console" | "trace" | "log";

const randomSeed = () => Math.floor(Math.random() * 1_000_000);
const splitArgs = (s: string) => s.split(/\s+/).filter(Boolean);
const REPO = "https://github.com/crowecawcaw/snappea";
const NARROW = "(max-width: 900px)";
/** How long the URL waits for edits to settle before it is rewritten. */
const URL_DEBOUNCE_MS = 500;
const TAB_LABEL: Record<Tab, string> = { files: "Files", console: "Console", trace: "Trace", log: "Log" };

export function App() {
  const runner = useMemo(() => new Runner(), []);
  const [demo, setDemo] = useState<Demo>(DEMOS[0]);
  const [files, setFiles] = useState<Files>({});
  const [open, setOpen] = useState("");
  const [tests, setTests] = useState<string[]>([]);
  const [testCase, setTestCase] = useState(DEMOS[0].testCase);
  const [timeout, setTimeoutSeconds] = useState(10);
  const [seed, setSeed] = useState(randomSeed);
  const [fixedSeed, setFixedSeed] = useState(false);
  const [extraArgs, setExtraArgs] = useState("");
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [compileConsole, setCompileConsole] = useState("");
  const [compileOk, setCompileOk] = useState<boolean | undefined>();
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CheckResult>();
  const [runError, setRunError] = useState<string>();
  const [progress, setProgress] = useState<Progress>();
  const [narrow, setNarrow] = useState(() => matchMedia(NARROW).matches);
  const [tab, setTab] = useState<Tab>(() => (matchMedia(NARROW).matches ? "files" : "console"));
  const [jump, setJump] = useState<{ line: number; nonce: number }>();
  const [loaded, setLoaded] = useState(false);
  const edited = useRef(false);
  const [blurbOpen, setBlurbOpen] = useState(false);
  const [blurbClamped, setBlurbClamped] = useState(false);
  const blurbRef = useRef<HTMLParagraphElement>(null);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [shareTooBig, setShareTooBig] = useState(false);
  const [dragging, setDragging] = useState(false);
  const dragDepth = useRef(0);
  const fileInput = useRef<HTMLInputElement>(null);

  /** Shows a project: a demo, or files the user opened or a link carried. */
  const applyProject = useCallback((d: Demo, s: Shared, isEdited: boolean) => {
    setDemo(d);
    setFiles(s.files);
    setOpen(s.files[s.open] !== undefined ? s.open : sortPaths(Object.keys(s.files))[0] ?? "");
    setTestCase(s.testCase ?? d.testCase);
    setTimeoutSeconds(s.timeout ?? 10);
    setExtraArgs(s.extraArgs ?? d.extraArgs ?? "");
    setResult(undefined);
    setRunError(undefined);
    setDiagnostics([]);
    setTab(matchMedia(NARROW).matches ? "files" : "console");
    edited.current = isEdited;
  }, []);

  const applyDemo = useCallback(
    (d: Demo, open = d.open) => applyProject(d, { demo: d.id, files: loadDemo(d), open }, false),
    [applyProject],
  );

  /** Loads what the URL describes: a shared project in the hash, else ?demo=<id>&file=<path>. */
  const loadFromUrl = useCallback(() => {
    const encoded = sharedFromHash();
    if (encoded) {
      try {
        const s = decodeShared(encoded);
        applyProject(DEMOS.find((d) => d.id === s.demo) ?? CUSTOM, s, true);
        return;
      } catch {
        alert("This link's project couldn't be read; it may have been cut off. Opening a demo instead.");
      }
    }
    const q = new URLSearchParams(location.search);
    const d = DEMOS.find((x) => x.id === q.get("demo")) ?? DEMOS[0];
    applyDemo(d, q.get("file") ?? d.open);
  }, [applyDemo, applyProject]);

  // Offer "more" only when the phone layout actually cuts the blurb off.
  useEffect(() => {
    const el = blurbRef.current;
    if (!el) return;
    const measure = () => setBlurbClamped(el.scrollHeight > el.clientHeight + 1);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [demo]);

  // Phones show the editor as a tab; wider screens show it beside the output.
  useEffect(() => {
    const mq = matchMedia(NARROW);
    const on = () => {
      setNarrow(mq.matches);
      if (!mq.matches) setTab((t) => (t === "files" ? "console" : t));
    };
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);

  // Initial load from the URL, and again when a pasted link changes only the hash.
  useEffect(() => {
    loadFromUrl();
    setLoaded(true);
    const onHash = () => {
      if (sharedFromHash()) loadFromUrl();
    };
    addEventListener("hashchange", onHash);
    return () => removeEventListener("hashchange", onHash);
  }, [loadFromUrl]);

  // Keep the URL in step with the project, once edits settle, so it can be shared.
  useEffect(() => {
    if (!loaded) return;
    const t = setTimeout(() => {
      const state: Shared = {
        demo: demo === CUSTOM ? undefined : demo.id,
        files,
        open,
        testCase,
        timeout,
        extraArgs: extraArgs || undefined,
      };
      const { url, tooBig } = urlFor(state, edited.current);
      setShareTooBig(tooBig);
      if (url !== location.search + location.hash) history.replaceState(null, "", url);
    }, URL_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [loaded, demo, files, open, testCase, timeout, extraArgs]);

  // Compile shortly after each edit: diagnostics and the list of test cases.
  useEffect(() => {
    if (!loaded || Object.keys(files).length === 0) return;
    let stale = false;
    const t = setTimeout(async () => {
      try {
        const r = await runner.compile(files);
        if (stale) return;
        setDiagnostics(r.diagnostics);
        setCompileConsole(r.console);
        setCompileOk(r.ok);
        if (r.ok) {
          setTests(r.tests);
          setTestCase((tc) => (r.tests.includes(tc) ? tc : r.tests[0] ?? ""));
        }
      } catch {
        /* cancelled */
      }
    }, 400);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [files, loaded, runner]);

  const check = useCallback(async () => {
    if (running || !testCase) return;
    const s = fixedSeed ? seed : randomSeed();
    setSeed(s);
    setRunning(true);
    setRunError(undefined);
    setProgress(undefined);
    try {
      const r = await runner.check(
        {
          files,
          testCase,
          timeout,
          seed: s,
          extraArgs: splitArgs(extraArgs),
        },
        setProgress,
      );
      setResult(r);
      if (r.diagnostics.length) setDiagnostics(r.diagnostics);
      setTab(r.bug && r.traceJson ? "trace" : "console");
    } catch (e) {
      const msg = String((e as Error).message ?? e);
      setRunError(msg === "cancelled" ? "Stopped." : `The checker crashed: ${msg}`);
      setResult(undefined);
      setTab("console");
    } finally {
      setRunning(false);
    }
  }, [running, testCase, fixedSeed, seed, runner, files, timeout, extraArgs]);

  const openLocation = useCallback(
    (file: string, line: number) => {
      const f = file.replace(/^\/work\//, "");
      const match = files[f] !== undefined ? f : Object.keys(files).find((k) => k.endsWith("/" + f) || k === f);
      if (!match) return;
      setOpen(match);
      setJump({ line, nonce: Date.now() });
      if (matchMedia(NARROW).matches) setTab("files");
    },
    [files],
  );

  const pickDemo = (id: string) => {
    if (edited.current && !confirm("Discard your edits and load another demo?")) return;
    applyDemo(DEMOS.find((x) => x.id === id)!);
  };

  /** Opens dropped or picked files as a new project. */
  const importFiles = async (read: Promise<Files>) => {
    let f: Files;
    try {
      f = await read;
    } catch (e) {
      alert(`Couldn't open those files: ${(e as Error).message ?? e}`);
      return;
    }
    if (edited.current && !confirm("Discard your edits and open these files?")) return;
    const paths = sortPaths(Object.keys(f));
    applyProject(CUSTOM, { files: f, open: paths.find((p) => p.endsWith(".p")) ?? paths[0] }, true);
  };

  const dropHandlers = {
    onDragEnter: (e: React.DragEvent) => {
      if (!e.dataTransfer.types.includes("Files")) return;
      e.preventDefault();
      dragDepth.current++;
      setDragging(true);
    },
    onDragOver: (e: React.DragEvent) => {
      if (!e.dataTransfer.types.includes("Files")) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    },
    onDragLeave: () => {
      if (--dragDepth.current <= 0) {
        dragDepth.current = 0;
        setDragging(false);
      }
    },
    onDrop: (e: React.DragEvent) => {
      if (!e.dataTransfer.types.includes("Files")) return;
      e.preventDefault();
      dragDepth.current = 0;
      setDragging(false);
      void importFiles(filesFromDrop(e.dataTransfer));
    },
  };

  const addFile = () => {
    const name = prompt("New file name", "PSrc/NewMachine.p")?.trim();
    if (!name) return;
    const path = name.endsWith(".p") || name.includes(".") ? name : `${name}.p`;
    if (files[path] === undefined) setFiles({ ...files, [path]: "" });
    setOpen(path);
    edited.current = true;
  };

  const paths = useMemo(() => sortPaths(Object.keys(files)), [files]);
  const errorsByFile = useMemo(() => {
    const m = new Map<string, number>();
    for (const d of diagnostics) m.set(d.file, (m.get(d.file) ?? 0) + 1);
    return m;
  }, [diagnostics]);

  const status = running
    ? progress
      ? progressText(progress)
      : "Checking…"
    : runError
      ? runError
      : compileOk === false || (result && !result.ok)
        ? "Compile error"
        : result
          ? result.bug
            ? "Bug found"
            : result.exitCode !== 0
              ? "Error (see console)"
              : "No bugs found (see console for completion)"
          : compileOk
            ? "Ready"
            : "Loading…";
  const statusClass = running ? "" : compileOk === false || result?.bug || (result && (!result.ok || result.exitCode !== 0)) ? "bad" : result?.ok ? "good" : "";

  const blurb = (
    <div className={blurbOpen ? "blurb open" : "blurb"}>
      <p ref={blurbRef}>{demo.blurb}</p>
      {(blurbClamped || blurbOpen) && (
        <button className="more" onClick={() => setBlurbOpen(!blurbOpen)}>
          {blurbOpen ? "less" : "more"}
        </button>
      )}
    </div>
  );
  const compileErr = !running && (compileOk === false || (result !== undefined && !result.ok)) && diagnostics.length > 0;
  // Phones: the tabs carry the result (Trace dot, console verdict); show the row only for
  // progress and errors.
  const showStatus = narrow ? running || !!runError || compileErr : running || !!result || !!runError || compileOk === false;
  const downloads = result?.schedule && !running && (
    <span className="downloads">
      <Download name={`${result.testCase}.schedule`} text={result.schedule} />
      {result.traceText && <Download name={`${result.testCase}.txt`} text={result.traceText} />}
      {result.traceJson && <Download name={`${result.testCase}.trace.json`} text={result.traceJson} />}
    </span>
  );

  return (
    <div className="app" {...dropHandlers}>
      {dragging && (
        <div className="drop-overlay" aria-hidden="true">
          <div>Drop .p files, a folder or a .zip to open them</div>
        </div>
      )}
      <header className="top">
        <a className="brand" href="./" aria-label="Snap Pea home">
          <Logo />
          <span className="brand-name">Playground</span>
        </a>
        <select className="demo-select" value={demo.id} onChange={(e) => pickDemo(e.target.value)} aria-label="Demo">
          {demo === CUSTOM && <option value={CUSTOM.id}>{CUSTOM.title}</option>}
          {DEMOS.map((d) => (
            <option key={d.id} value={d.id}>
              {d.title}
            </option>
          ))}
        </select>
        <button
          className="ghost open-files"
          onClick={() => fileInput.current?.click()}
          title="Open .p files or a .zip (or drop them anywhere)"
        >
          Open
        </button>
        <input
          ref={fileInput}
          type="file"
          multiple
          accept=".p,.zip,.toml,.json,.txt,.md"
          hidden
          onChange={(e) => {
            const list = e.target.files;
            if (list?.length) void importFiles(filesFromList(list));
            e.target.value = "";
          }}
        />
        {shareTooBig && (
          <span
            className="share-note"
            title={`The project is over ${MAX_SHARE_CHARS / 1000}k characters compressed, so the page URL no longer holds your edits.`}
          >
            Too big to link
          </span>
        )}
        <div className="spacer" />
        <a className="button ghost docs-link" href="./docs/overview.html">
          Docs
        </a>
        <a className="button ghost gh" href={REPO} target="_blank" rel="noreferrer" aria-label="GitHub">
          <svg width="20" height="20" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
          </svg>
          <span className="gh-text">GitHub</span>
        </a>
      </header>

      {!narrow && blurb}

      <main className={`panes tab-${tab}`}>
        <section className="pane editor-pane">
          {narrow && blurb}
          <div className="file-tabs" role="tablist">
            {paths.map((p) => {
              const slash = p.lastIndexOf("/");
              const errs = errorsByFile.get(p);
              return (
                <button
                  key={p}
                  role="tab"
                  aria-selected={p === open}
                  className={p === open ? "file-tab active" : "file-tab"}
                  onClick={() => setOpen(p)}
                  title={p}
                >
                  {slash >= 0 && <span className="dir">{p.slice(0, slash + 1)}</span>}
                  {p.slice(slash + 1)}
                  {errs ? <span className="badge">{errs}</span> : null}
                </button>
              );
            })}
            <button className="file-tab add" onClick={addFile} title="New file" aria-label="New file">
              +
            </button>
          </div>
          {open && files[open] !== undefined && (
            <PEditor
              path={open}
              value={files[open]}
              diagnostics={diagnostics.filter((d) => d.file === open)}
              jump={jump}
              onChange={(p, text) => {
                edited.current = true;
                setFiles((f) => ({ ...f, [p]: text }));
              }}
              onRun={check}
            />
          )}
        </section>

        <section className="pane run-pane">
          <div className={optionsOpen ? "controls options-open" : "controls"}>
            <div className="run-bar">
              <label className="tc">
                <span className="lbl">Test case</span>
                <select
                  value={testCase}
                  onChange={(e) => setTestCase(e.target.value)}
                  disabled={!tests.length}
                  aria-label="Test case"
                >
                  {tests.length === 0 && <option value={testCase}>{testCase || "—"}</option>}
                  {tests.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </label>
              {running ? (
                <button className="danger run" onClick={() => runner.cancel()}>
                  Stop
                </button>
              ) : (
                <button className="primary run" onClick={check} disabled={!testCase || compileOk === false} title="Ctrl/⌘+Enter">
                  {compileOk !== false
                    ? "Check"
                    : diagnostics.length === 1
                      ? "Fix 1 error"
                      : diagnostics.length
                        ? `Fix ${diagnostics.length} errors`
                        : "Fix errors"}
                </button>
              )}
            </div>
            <details className="options" open={optionsOpen} onToggle={(e) => setOptionsOpen((e.target as HTMLDetailsElement).open)}>
              <summary>
                <svg className="chevron" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                  <path d="M3 1.5 L6.5 5 L3 8.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                Options
                <span className="muted">
                  {" "}· up to {timeout}s
                  {fixedSeed ? ` · seed ${seed}` : ""}
                  {extraArgs.trim() ? ` · ${extraArgs.trim()}` : ""}
                </span>
              </summary>
              <div className="options-grid">
                <label>
                  Timeout (seconds)
                  <input
                    type="number"
                    inputMode="numeric"
                    min={1}
                    max={3600}
                    value={timeout}
                    onChange={(e) => setTimeoutSeconds(Math.min(3600, Math.max(1, Math.trunc(Number(e.target.value)) || 1)))}
                  />
                </label>
                <label>
                  Seed
                  <input
                    type="number"
                    inputMode="numeric"
                    value={seed}
                    onChange={(e) => {
                      setSeed(Number(e.target.value) || 0);
                      setFixedSeed(true);
                    }}
                  />
                </label>
                <label className="check">
                  <input type="checkbox" checked={fixedSeed} onChange={(e) => setFixedSeed(e.target.checked)} />
                  Keep seed
                </label>
                <label className="span-all">
                  Extra flags
                  <input
                    value={extraArgs}
                    placeholder="e.g. --timed"
                    onChange={(e) => setExtraArgs(e.target.value)}
                    spellCheck={false}
                    autoCapitalize="off"
                    autoCorrect="off"
                  />
                </label>
              </div>
            </details>
          </div>

          <div className="out-head">
            <div className="out-tabs" role="tablist">
              {((narrow ? ["files", "console", "trace", "log"] : ["console", "trace", "log"]) as Tab[]).map((t) => (
                <button
                  key={t}
                  role="tab"
                  aria-selected={tab === t}
                  className={tab === t ? "out-tab active" : "out-tab"}
                  onClick={() => setTab(t)}
                  disabled={(t === "trace" || t === "log") && !result?.traceJson}
                >
                  {TAB_LABEL[t]}
                  {t === "files" && diagnostics.length > 0 && <span className="badge">{diagnostics.length}</span>}
                  {t === "trace" && result?.bug && <span className="bug-dot" aria-label="bug found" />}
                </button>
              ))}
              {narrow && (
                <button
                  className={optionsOpen ? "out-tab gear active" : "out-tab gear"}
                  onClick={() => setOptionsOpen(!optionsOpen)}
                  aria-expanded={optionsOpen}
                  aria-label="Checker options"
                  title="Checker options"
                >
                  <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0" />
                    <circle cx="16" cy="6" r="2" />
                    <circle cx="10" cy="12" r="2" />
                    <circle cx="18" cy="18" r="2" />
                  </svg>
                </button>
              )}
            </div>
            {showStatus && (
            <div className="status-line">
              <span className={`status ${statusClass}`}>{status}</span>
              {running && !!progress?.time_limit && (
                <span className="progress" role="progressbar" aria-label="Time used" aria-valuemin={0} aria-valuemax={100}
                  aria-valuenow={Math.round(Math.min(1, progress.elapsed / progress.time_limit) * 100)}>
                  <span style={{ width: `${Math.min(1, progress.elapsed / progress.time_limit) * 100}%` }} />
                </span>
              )}
              {!running && (compileOk === false || (result && !result.ok)) && diagnostics[0] && (
                <button className="diag" onClick={() => openLocation(diagnostics[0].file, diagnostics[0].line)}>
                  {diagnostics[0].file}:{diagnostics[0].line} {diagnostics[0].message}
                </button>
              )}
              {result && !running && !compileErr && <span className="muted">{(result.millis / 1000).toFixed(2)} s</span>}
              {!narrow && downloads}
            </div>
            )}
          </div>

          <div className="output">
            {tab === "console" && (
              <Console
                summary={!runError && result?.ok && (result.bug || result.exitCode === 0) ? (result.bug ? `✕ ${bugLine(result)}` : `✓ No bugs found in ${result.testCase}`) : undefined}
                bad={result?.bug}
                text={runError ?? (result ? result.console : compileOk === false ? compileConsole : "")}
                onOpenLocation={openLocation}
                empty={compileOk === false ? "" : "Press Check (Ctrl/⌘+Enter) to explore schedules of the selected test case."}
              />
            )}
            {(tab === "trace" || tab === "log") && result?.traceJson && (
              <TraceView
                json={result.traceJson}
                view={tab === "log" ? "log" : "diagram"}
                onOpenLocation={openLocation}
                onShowDiagram={() => setTab("trace")}
              />
            )}
          </div>
        </section>
      </main>
    </div>
  );
}

const LOC_G = /([\w./-]+\.p):(\d+)(?::(\d+))?/g;

/** The checker's one-line verdict for a buggy run, from its console output. */
function bugLine(r: CheckResult): string {
  const line = `${r.traceText ?? ""}\n${r.console}`.split("\n").find((l) => /<ErrorLog>|Assertion Failed|Deadlock|liveness|Exception/i.test(l));
  return line ? line.replace(/^.*?<ErrorLog>\s*/, "").trim() : "Bug found";
}

function Console({ text, empty, summary, bad, onOpenLocation }: {
  text: string; empty?: string; summary?: string; bad?: boolean; onOpenLocation: (f: string, l: number) => void;
}) {
  if (!text) return <div className="console empty">{empty}</div>;
  return (
    <pre className="console">
      {summary && <div className={bad ? "summary bad" : "summary good"}>{summary}</div>}
      {text.split("\n").map((line, i) => {
        const cls =
          /^FAIL\s|\bfailed in\b|Error|Assertion Failed|\bbug found\b|found a bug|Found [1-9]/i.test(line)
            ? "line bad"
            : /^ok\s|\bpassed in\b|Found 0 bug/.test(line)
              ? "line good"
              : line.startsWith("$ ")
                ? "line cmd"
                : "line";
        const parts: React.ReactNode[] = [];
        let last = 0;
        for (const m of line.matchAll(LOC_G)) {
          parts.push(line.slice(last, m.index));
          const [all, file, ln] = m;
          parts.push(
            <button key={m.index} className="loc" onClick={() => onOpenLocation(file, Number(ln))}>
              {all}
            </button>,
          );
          last = (m.index ?? 0) + all.length;
        }
        parts.push(line.slice(last));
        return (
          <div key={i} className={cls}>
            {parts}
          </div>
        );
      })}
    </pre>
  );
}

function Download({ name, text }: { name: string; text: string }) {
  const href = useMemo(() => URL.createObjectURL(new Blob([text], { type: "text/plain" })), [text]);
  useEffect(() => () => URL.revokeObjectURL(href), [href]);
  return (
    <a className="download" href={href} download={name} title={`Download ${name}`}>
      ⤓ {name.slice(name.indexOf(".") + 1)}
    </a>
  );
}

/** A running check in a few words, e.g. `Checking tcMain: 1,240 schedules · 3.1 of 10 s`. */
function progressText(p: Progress): string {
  const time = p.time_limit ? `${p.elapsed.toFixed(1)} of ${p.time_limit} s` : `${p.elapsed.toFixed(1)} s`;
  if (p.phase === "mutation") return `Mutation testing: ${p.detected} of ${p.mutants} mutants detected · ${time}`;
  const n = p.schedules.toLocaleString("en-US");
  const bugs = p.bugs ? ` · ${p.bugs} bug${p.bugs === 1 ? "" : "s"}` : "";
  return `Checking ${p.test_case}: ${n} schedule${p.schedules === 1 ? "" : "s"}${bugs} · ${time}`;
}
