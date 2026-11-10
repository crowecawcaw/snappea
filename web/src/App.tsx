import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CUSTOM, DEMOS, loadDemo, sortPaths, type Demo } from "./demos";
import { filesFromDrop, filesFromList } from "./importFiles";
import { decodeShared, sharedFromHash, urlFor, MAX_SHARE_CHARS, type Shared } from "./share";
import { PEditor } from "./editor/PEditor";
import type { CheckResult, Diagnostic, Files, Progress } from "./protocol";
import { Runner } from "./runner";
import { ScheduleExplorer } from "./trace/ScheduleExplorer";
import { ModelOverview } from "./model/ModelOverview";
import type { ModelInfo } from "./model/types";
import { Icon, type IconName } from "./ui/Icon";
import { Logo } from "./site/Logo";
import { Console, CONSOLE_HELP, splitCommand, type ConsoleEntry } from "./Console";

/** Main tabs. "files" is the editor, which is a tab only on phones. */
type Tab = "files" | "model" | "test" | "console";
/** The two views of a recorded execution in the Test tab. */
type TestView = "replay" | "sequence";

const randomSeed = () => Math.floor(Math.random() * 1_000_000);
const splitArgs = (s: string) => s.split(/\s+/).filter(Boolean);
const REPO = "https://github.com/crowecawcaw/snappea";
const NARROW = "(max-width: 900px)";
/** How long the URL waits for edits to settle before it is rewritten. */
const URL_DEBOUNCE_MS = 500;
const TAB_LABEL: Record<Tab, string> = { files: "Files", model: "Model", test: "Test", console: "Console" };
const TAB_ICON: Record<Tab, IconName> = { files: "code", model: "model", test: "check", console: "console" };
/** Editor width as a fraction of the window; the divider snaps to the center near it. */
const SPLIT_KEY = "playground.split";
const SNAP = 0.03;

function savedSplit(): number {
  try {
    const v = Number(localStorage.getItem(SPLIT_KEY));
    return v >= 0.2 && v <= 0.8 ? v : 0.5;
  } catch {
    return 0.5;
  }
}

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
  const [strict, setStrict] = useState(false);
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [model, setModel] = useState<ModelInfo>();
  const [modelKey, setModelKey] = useState("");
  const [resultKey, setResultKey] = useState("");
  const [resultModel, setResultModel] = useState<ModelInfo>();
  const sourceKey = JSON.stringify(files);
  const originalFiles = useMemo(() => demo === CUSTOM ? undefined : loadDemo(demo), [demo]);
  const changedExample = originalFiles && (Object.keys(originalFiles).length !== Object.keys(files).length || Object.entries(originalFiles).some(([path, text]) => files[path] !== text));
  const [compileOk, setCompileOk] = useState<boolean | undefined>();
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CheckResult>();
  const [runError, setRunError] = useState<string>();
  const [progress, setProgress] = useState<Progress>();
  const [narrow, setNarrow] = useState(() => matchMedia(NARROW).matches);
  const [tab, setTab] = useState<Tab>(() => (matchMedia(NARROW).matches ? "files" : "model"));
  const [testView, setTestView] = useState<TestView>("replay");
  const [split, setSplit] = useState(savedSplit);
  const panesRef = useRef<HTMLElement>(null);
  const [log, setLog] = useState<ConsoleEntry[]>([]);
  const logId = useRef(0);
  const [cmdRunning, setCmdRunning] = useState(false);
  const print = useCallback((text: string) => setLog((l) => [...l, { id: ++logId.current, text }]), []);
  const [jump, setJump] = useState<{ line: number; nonce: number; focus?: boolean }>();
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
    setStrict(s.strict ?? false);
    setResult(undefined);
    setRunError(undefined);
    setDiagnostics([]);
    setModel(undefined);
    setModelKey("");
    setTab(matchMedia(NARROW).matches ? "files" : "model");
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
      if (!mq.matches) setTab((t) => (t === "files" ? "model" : t));
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
        strict: strict || undefined,
      };
      const { url, tooBig } = urlFor(state, edited.current);
      setShareTooBig(tooBig);
      if (url !== location.search + location.hash) history.replaceState(null, "", url);
    }, URL_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [loaded, demo, files, open, testCase, timeout, extraArgs, strict]);

  // Compile shortly after each edit: diagnostics and the list of test cases.
  useEffect(() => {
    if (!loaded || Object.keys(files).length === 0) return;
    let stale = false;
    const t = setTimeout(async () => {
      try {
        const r = await runner.compile(files);
        if (stale) return;
        setDiagnostics(r.diagnostics);
        setCompileOk(r.ok);
        setModel(r.model);
        setModelKey(JSON.stringify(files));
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
    setTab("test");
    try {
      const compiled = await runner.compile(files);
      const r = await runner.check(
        {
          files,
          testCase,
          timeout,
          seed: s,
          strict,
          extraArgs: splitArgs(extraArgs),
        },
        setProgress,
      );
      setResult(r);
      setResultKey(JSON.stringify(files));
      setResultModel(compiled.model);
      if (r.diagnostics.length) setDiagnostics(r.diagnostics);
      print(r.console);
    } catch (e) {
      const msg = String((e as Error).message ?? e);
      const text = msg === "cancelled" ? "Stopped." : `The checker crashed: ${msg}`;
      setRunError(text);
      setResult(undefined);
      print(text);
    } finally {
      setRunning(false);
    }
  }, [running, testCase, fixedSeed, seed, runner, files, timeout, strict, extraArgs, print]);

  /** Runs a line typed in the console: a snappea command, or ls, cat, clear, help. */
  const runCommand = useCallback(async (line: string) => {
    const words = splitCommand(line);
    if (words[0] === "snappea") words.shift();
    const [cmd, ...rest] = words;
    if (cmd === "clear") return setLog([]);
    const echo = `$ snappea ${words.join(" ")}`;
    if (cmd === undefined) return;
    if (cmd === "help") return print(`$ help\n${CONSOLE_HELP}`);
    if (cmd === "ls") return print(`$ ls\n${sortPaths(Object.keys(files)).join("\n")}`);
    if (cmd === "cat") {
      const f = rest[0] && (files[rest[0]] !== undefined ? rest[0] : Object.keys(files).find((k) => k.endsWith("/" + rest[0])));
      return print(`$ cat ${rest.join(" ")}\n${f ? files[f].trimEnd() : `cat: ${rest[0] ?? "<file>"}: no such file. Type ls to list files.`}`);
    }
    if (running || cmdRunning) return print(`${echo}\nA check is already running. Stop it first.`);
    setCmdRunning(true);
    try {
      const r = await runner.run(files, words);
      print(r.console ? `${echo}\n${r.console}` : echo);
    } catch (e) {
      const msg = String((e as Error).message ?? e);
      print(`${echo}\n${msg === "cancelled" ? "Stopped." : `The checker crashed: ${msg}`}`);
    } finally {
      setCmdRunning(false);
    }
  }, [files, print, running, cmdRunning, runner]);

  /** Drags the editor/output divider; it snaps to the center when close to it. */
  const dragSplit = (e: React.PointerEvent) => {
    const box = panesRef.current?.getBoundingClientRect();
    if (!box) return;
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("resizing");
    let last = split;
    const move = (ev: PointerEvent) => {
      let f = (ev.clientX - box.left) / box.width;
      f = Math.min(0.8, Math.max(0.2, f));
      if (Math.abs(f - 0.5) < SNAP) f = 0.5;
      last = f;
      setSplit(f);
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      document.body.classList.remove("resizing");
      try {
        localStorage.setItem(SPLIT_KEY, String(last));
      } catch {
        /* storage unavailable */
      }
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  };
  const nudgeSplit = (f: number) => {
    const v = Math.abs(f - 0.5) < SNAP / 2 ? 0.5 : Math.min(0.8, Math.max(0.2, f));
    setSplit(v);
    try {
      localStorage.setItem(SPLIT_KEY, String(v));
    } catch {
      /* storage unavailable */
    }
  };

  const openLocation = useCallback(
    (file: string, line: number, focus = true) => {
      const f = file.replace(/^\/work\//, "");
      const match = files[f] !== undefined ? f : Object.keys(files).find((k) => k.endsWith("/" + f) || k === f);
      if (!match) return;
      setOpen(match);
      setJump({ line, nonce: Date.now(), focus });
      if (focus && matchMedia(NARROW).matches) setTab("files");
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

  const staleResult = !!result && (resultKey !== sourceKey || result.testCase !== testCase);
  const status = running
    ? progress
      ? progressText(progress)
      : "Checking…"
    : runError
      ? runError
      : compileOk === false || (result && !result.ok)
        ? "Compile error"
        : staleResult
          ? "Previous run · source or test changed"
          : result
          ? result.bug
            ? "Bug found"
            : result.exitCode !== 0
              ? "Error (see console)"
              : "No bugs found (see console for completion)"
          : compileOk
            ? "Ready"
            : "Loading…";
  const statusClass = running || staleResult ? "" : compileOk === false || result?.bug || (result && (!result.ok || result.exitCode !== 0)) ? "bad" : result?.ok ? "good" : "";

  const blurb = (
    <div className={blurbOpen ? "blurb open" : "blurb"}>
      <p ref={blurbRef}>{changedExample ? `You are editing “${demo.title}”. Explore the current source in Model, or Check again to find and replay its executions.` : demo.blurb}</p>
      {(blurbClamped || blurbOpen) && (
        <button className="more" onClick={() => setBlurbOpen(!blurbOpen)}>
          {blurbOpen ? "less" : "more"}
        </button>
      )}
    </div>
  );
  const compileErr = !running && (compileOk === false || (result !== undefined && !result.ok)) && diagnostics.length > 0;
  const showStatus = running || !!result || !!runError || compileOk === false;
  const downloads = result?.schedule && !running && !staleResult && (
    <span className="downloads">
      <Download name={`${result.testCase}.schedule`} text={result.schedule} />
      {result.traceText && <Download name={`${result.testCase}.txt`} text={result.traceText} />}
      {result.traceJson && <Download name={`${result.testCase}.trace.json`} text={result.traceJson} />}
    </span>
  );
  const tabs: Tab[] = narrow ? ["files", "model", "test", "console"] : ["model", "test", "console"];
  const fileName = (p: string) => p.slice(p.lastIndexOf("/") + 1);
  const duplicateNames = new Set(paths.map(fileName).filter((n, i, all) => all.indexOf(n) !== i));

  const controls = (
    <div className={optionsOpen ? "controls options-open" : "controls"}>
      <div className="run-bar">
        <label className="tc">
          <span className="lbl">Test case</span>
          <select value={testCase} onChange={(e) => setTestCase(e.target.value)} disabled={!tests.length} aria-label="Test case">
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
          <button className="primary run" onClick={check} disabled={!testCase || compileOk === false || cmdRunning} title="Ctrl/⌘+Enter">
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
            {strict ? " · strict" : ""}
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
          <label className="check" title="Fail on lint findings and missing coverage, as the snappea CLI does. Off: they are shown as warnings and the check goes on.">
            <input type="checkbox" checked={strict} onChange={(e) => setStrict(e.target.checked)} />
            Strict checks
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
  );

  const statusLine = showStatus && (
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
      {!narrow && !result?.schedules && downloads}
    </div>
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
          Open files
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

      <main
        ref={panesRef}
        className={`panes tab-${tab}`}
        style={narrow ? undefined : { gridTemplateColumns: `minmax(0, ${split}fr) 0 minmax(0, ${1 - split}fr)` }}
      >
        <section className="pane editor-pane">
          {narrow && blurb}
          <div className="file-tabs" role="tablist" aria-label="Source files">
            {paths.map((p) => {
              const errors = errorsByFile.get(p) ?? 0;
              const name = fileName(p);
              return (
                <button
                  key={p}
                  role="tab"
                  aria-selected={p === open}
                  className={p === open ? "file-tab active" : "file-tab"}
                  title={errors ? `${p} · ${errors} error${errors === 1 ? "" : "s"}` : p}
                  onClick={() => setOpen(p)}
                >
                  <span className={errors ? "file-name has-errors" : "file-name"}>{name}</span>
                  {duplicateNames.has(name) && p.includes("/") && <span className="dir">{p.slice(0, p.lastIndexOf("/"))}</span>}
                  {errors > 0 && <span className="file-errors" aria-label={`${errors} errors`}>{errors}</span>}
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

        {!narrow && (
          <div
            className="divider"
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize editor and results"
            aria-valuemin={20}
            aria-valuemax={80}
            aria-valuenow={Math.round(split * 100)}
            tabIndex={0}
            title="Drag to resize. Double-click to center."
            onPointerDown={dragSplit}
            onDoubleClick={() => nudgeSplit(0.5)}
            onKeyDown={(e) => {
              if (e.key === "ArrowLeft") nudgeSplit(split - 0.02);
              else if (e.key === "ArrowRight") nudgeSplit(split + 0.02);
              else return;
              e.preventDefault();
            }}
          >
            <i aria-hidden="true" />
          </div>
        )}

        <section className="pane run-pane">
          <div className="out-head">
            <div className="out-tabs" role="tablist">
              {tabs.map((t) => (
                <button
                  key={t}
                  role="tab"
                  aria-selected={tab === t}
                  className={tab === t ? "out-tab active" : "out-tab"}
                  onClick={() => setTab(t)}
                >
                  <Icon name={TAB_ICON[t]} />
                  {TAB_LABEL[t]}
                  {t === "files" && diagnostics.length > 0 && <span className="badge">{diagnostics.length}</span>}
                  {t === "test" && result?.bug && !staleResult && <span className="bug-dot" aria-label="bug found" />}
                  {t === "test" && running && <span className="run-dot" aria-label="checking" />}
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
          </div>
          {narrow && controls}

          <div className={`output output-${tab}`}>
            {tab === "model" && <ModelOverview model={modelKey === sourceKey ? model : undefined} testCase={testCase} pending={modelKey !== sourceKey} onOpen={s => openLocation(s.file, s.line)} />}
            {tab === "test" && (
              <div className="test-pane">
                {!narrow && controls}
                {!(result?.traceJson && !running) && statusLine}
                {result?.traceJson && !running ? (
                  <ScheduleExplorer
                    json={result.traceJson}
                    schedules={result.schedules}
                    bug={result.bug}
                    downloads={downloads}
                    status={statusLine}
                    model={resultModel}
                    stale={staleResult}
                    view={testView === "sequence" ? "messages" : "diagram"}
                    onViewChange={(v) => setTestView(v === "messages" ? "sequence" : "replay")}
                    onOpenLocation={openLocation}
                  />
                ) : (
                  !running && (
                    <div className="test-empty">
                      {compileOk === false ? (
                        <p>Fix the errors marked in the editor, then run Check.</p>
                      ) : result || runError ? (
                        <p>
                          This run recorded no execution to replay.{" "}
                          <button className="link" onClick={() => setTab("console")}>Open Console</button> for the checker's output.
                        </p>
                      ) : (
                        <>
                          <p>
                            <strong>Check</strong> runs the selected test case over many schedules and looks for a failing one.
                          </p>
                          <p className="muted">
                            Then step through it here, as a replay or as a sequence diagram. Shortcut: Ctrl/⌘+Enter in the editor.
                          </p>
                        </>
                      )}
                    </div>
                  )
                )}
              </div>
            )}
            {tab === "console" && (
              <Console
                entries={log}
                busy={cmdRunning}
                onCommand={runCommand}
                onStop={() => runner.cancel()}
                onOpenLocation={openLocation}
              />
            )}
          </div>
        </section>
      </main>
    </div>
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
