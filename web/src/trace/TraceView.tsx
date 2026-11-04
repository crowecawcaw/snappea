import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ReplayStory } from "./ReplayStory";
import { SystemDiagram } from "../model/SystemDiagram";
import type { ModelInfo } from "../model/types";
import { explainStep, framesAt } from "./explain";
import { visible } from "./filter";
import { parseTrace, type Trace } from "./model";
import { laneColors, SequenceDiagram } from "./SequenceDiagram";

export interface Props {
  json: string;
  model?: ModelInfo;
  stale?: boolean;
  /** "diagram": the chart (plus the step list beside it when there's room); "log": the step list alone. */
  view: "diagram" | "log";
  onOpenLocation: (file: string, line: number, focus?: boolean) => void;
  /** Picking a step in the log view asks to show it in the diagram. */
  onShowDiagram: () => void;
}

const LOC = /([\w./-]+\.p):(\d+):(\d+)/;
/** Show the step list beside the diagram when the trace panel is at least this wide… */
const SIDE_LIST_MIN = 820;
/** …and below it when the panel is at least this tall (phones have the Log tab instead). */
const BOTTOM_LIST_MIN_H = 560;

/** Bug trace: step-through controls, machine focus, the diagram, and the step list. */
export function TraceView({ json, view, model, stale, onOpenLocation, onShowDiagram }: Props) {
  const trace = useMemo(() => parseTrace(json), [json]);
  const color = useMemo(() => laneColors(trace.lanes), [trace]);
  const [cursor, setCursor] = useState(trace.steps.length - 1);
  const [mode, setMode] = useState<"story" | "sequence" | "states">(model ? "states" : "story");
  const [follow, setFollow] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [bannerOpen, setBannerOpen] = useState(false);
  const [focus, setFocus] = useState<ReadonlySet<number>>(new Set());
  const [hover, setHover] = useState<number>();
  const root = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<"side" | "bottom" | "none">("none");

  const { rows: allRows, cols } = useMemo(() => visible(trace, focus), [trace, focus]);
  // Source snapshots are useful in the story; keep the communication diagram compact.
  const rows = useMemo(() => mode === "sequence" ? allRows.filter(i => !trace.steps[i].inspection) : mode === "states" && view !== "log" ? allRows.filter(i => !trace.steps[i].inspection || trace.steps[i].inspection?.write) : allRows, [allRows, mode, trace, view]);
  const pos = rows.indexOf(cursor);
  const lastPos = rows.length - 1;

  // A new trace: start at the bug, with no focus.
  useEffect(() => {
    setCursor(trace.steps.length - 1);
    setPlaying(false);
    setFocus(new Set());
  }, [trace]);

  // Focusing can hide the current step: move to the nearest earlier one still shown.
  useEffect(() => {
    if (pos >= 0 || !rows.length) return;
    const before = rows.filter((i) => i < cursor);
    setCursor(before.length ? before[before.length - 1] : rows[0]);
  }, [pos, rows, cursor]);

  useEffect(() => {
    if (!playing) return;
    if (pos >= lastPos) {
      setPlaying(false);
      return;
    }
    const t = setTimeout(() => setCursor(rows[Math.min(pos + 1, lastPos)]), 1100);
    return () => clearTimeout(t);
  }, [playing, pos, lastPos, rows]);

  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      setPlace(el.clientWidth >= SIDE_LIST_MIN ? "side" : el.clientHeight >= BOTTOM_LIST_MIN_H && !matchMedia("(max-width: 900px)").matches ? "bottom" : "none"),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const go = (i: number | undefined) => {
    if (i === undefined) return;
    setPlaying(false);
    setCursor(i);
  };
  const toPos = (p: number) => go(rows[Math.max(0, Math.min(p, lastPos))]);
  const toggleLane = (l: number) =>
    setFocus((f) => {
      const n = new Set(f);
      if (n.has(l)) n.delete(l);
      else n.add(l);
      return n;
    });

  const step = trace.steps[cursor];
  const source = step?.inspection?.source;
  useEffect(() => {
    if (follow && source && !stale) onOpenLocation(source.file, source.line, false);
  }, [follow, source, stale, onOpenLocation]);
  const frames = useMemo(() => framesAt(trace, cursor), [trace, cursor]);
  const actor = step?.lane === undefined ? undefined : frames.get(step.lane);
  const loc = trace.error ? LOC.exec(trace.error) : null;
  const list = (
    <StepList
      trace={trace}
      rows={rows}
      color={color}
      cursor={cursor}
      hover={hover}
      onSelect={(i) => {
        go(i);
        if (view === "log") onShowDiagram();
      }}
      onHover={setHover}
    />
  );

  const controls = (
        <div className="trace-controls">
          <button onClick={() => toPos(0)} title="First step" aria-label="First step" className="jump">⏮</button>
          <button onClick={() => toPos(pos - 1)} title="Previous step" aria-label="Previous step">◀</button>
          <button
            className="primary"
            onClick={() => {
              if (!playing && pos >= lastPos) setCursor(rows[0]);
              setPlaying(!playing);
            }}
            title={playing ? "Pause" : "Play"}
          >
            {playing ? "Pause" : "Play"}
          </button>
          <button onClick={() => toPos(pos + 1)} title="Next step" aria-label="Next step">▶</button>
          <button onClick={() => toPos(lastPos)} title="Last step" aria-label="Last step" className="jump">⏭</button>
          <input
            type="range"
            min={0}
            max={Math.max(lastPos, 0)}
            value={Math.max(pos, 0)}
            onChange={(e) => toPos(Number(e.target.value))}
            aria-label="Step"
          />
          <span className="muted">
            {pos + 1}/{rows.length}
          </span>
        </div>
  );

  return (
    <div className="trace" ref={root}>
      {stale && <div className="stale-trace" role="status">Source or test changed. This is the previous execution. Check again to replay your edits; source links are paused.</div>}
      {view === "diagram" && <div className="replay-toolbar"><div className="view-switch" aria-label="Replay view">
        <button aria-pressed={mode === "states"} onClick={() => setMode("states")} disabled={!model}>State diagram</button>
        <button aria-pressed={mode === "story"} onClick={() => setMode("story")}>Details</button>
        <button aria-pressed={mode === "sequence"} onClick={() => setMode("sequence")}>Messages</button>
      </div><label className="follow-source"><input type="checkbox" checked={follow} disabled={stale} onChange={e => setFollow(e.target.checked)} />Follow source</label></div>}
      {trace.error && mode !== "states" && (
        <div className={bannerOpen ? "trace-error open" : "trace-error"} onClick={() => setBannerOpen(!bannerOpen)}>
          <span className="trace-error-text">
            <strong>Bug:</strong> {trace.error}
          </span>
          {loc && (
            <button
              className="link"
              disabled={stale}
              onClick={(e) => {
                e.stopPropagation();
                if (!stale) onOpenLocation(loc[1], Number(loc[2]));
              }}
            >
              <span className="goto-long">Go to {loc[1]}:{loc[2]}</span>
              <span className="goto-short">line {loc[2]} →</span>
            </button>
          )}
        </div>
      )}
      {view === "diagram" && controls}
      {focus.size > 0 && (
        <div className="trace-focus">
          <span className="muted">Only steps of</span>
          {[...focus].map((l) => (
            <button key={l} className="chip" style={{ "--c": color[l] } as React.CSSProperties} onClick={() => toggleLane(l)} aria-label={`Stop focusing ${trace.lanes[l].id}`}>
              <span className="chip-name">{trace.lanes[l].id}</span> <span aria-hidden="true">✕</span>
            </button>
          ))}
          <button className="link" onClick={() => setFocus(new Set())}>
            Show all
          </button>
        </div>
      )}
      {view === "log" ? (
        list
      ) : mode === "story" ? <ReplayStory sourceAvailable={!stale} trace={trace} cursor={cursor} onSelect={go} onOpen={s => { if (!stale) onOpenLocation(s.file, s.line); }} />
        : mode === "states" && model ? <div className="replay-state-panel">
          {step?.kind === "error" && <div className={`state-replay-summary ${step.kind === "error" ? "failure" : ""}`} aria-live="polite">
            {step.kind === "error" && <strong>Check failed</strong>}
            <p>{step.kind === "error" ? (trace.error ?? step.label).replace(/^Assertion Failed:\s*[^\s]+\.p:\d+:\d+\s*/, "") : explainStep(trace, step)}</p>
            {step.kind === "error" && loc && <button className="link" disabled={stale} onClick={() => onOpenLocation(loc[1], Number(loc[2]))}>Open failed assertion</button>}
          </div>}
          <SystemDiagram replayControls={controls} model={model} trace={trace} cursor={cursor} readOnly={stale} onOpen={s => { if (!stale) onOpenLocation(s.file, s.line); }} />
          {actor?.snapshot && <details className="model-reference"><summary>Variables in {trace.lanes[step.lane!].label}</summary><pre className="replay-code">{Object.entries({...actor.snapshot.fields, ...actor.snapshot.locals}).map(([name, value]) => `${name} = ${value}`).join("\n")}</pre><p className="capture-note">Last captured at step {actor.at! + 1}.</p></details>}
        </div> : (
        <>
          <div className={`trace-main list-${place}`}>
            <SequenceDiagram
              trace={trace}
              rows={rows}
              cols={cols}
              color={color}
              focus={focus}
              cursor={cursor}
              hover={hover}
              onSelect={go}
              onHover={setHover}
              onToggleLane={toggleLane}
            />
            {place !== "none" && list}
          </div>
          {step && place === "none" && (
            <div className="step-detail">
              <div>{step.log}</div>
              {step.payload !== undefined && <pre>{JSON.stringify(step.payload, null, 2)}</pre>}
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** Every shown step's log line; linked to the diagram by cursor and hover. */
function StepList({ trace, rows, color, cursor, hover, onSelect, onHover }: {
  trace: Trace;
  rows: number[];
  color: string[];
  cursor: number;
  hover?: number;
  onSelect: (i: number) => void;
  onHover: (i: number | undefined) => void;
}) {
  const box = useRef<HTMLOListElement>(null);

  // Keep the current step in view, scrolling only the list.
  useEffect(() => {
    const el = box.current;
    const li = el?.querySelector<HTMLElement>(`[data-step="${cursor}"]`);
    if (!el || !li) return;
    const top = li.offsetTop - el.offsetTop;
    if (top < el.scrollTop) el.scrollTop = top - 8;
    else if (top + li.offsetHeight > el.scrollTop + el.clientHeight) el.scrollTop = top + li.offsetHeight - el.clientHeight + 8;
  }, [cursor, rows]);

  return (
    <ol className="step-list" ref={box} aria-label="Steps">
      {rows.map((i) => {
        const s = trace.steps[i];
        const cls = [
          i === cursor && "current",
          i === hover && "hover",
          s.kind === "error" && "error",
        ]
          .filter(Boolean)
          .join(" ");
        return (
          <li key={i} data-step={i} className={cls}>
            <button
              onClick={() => onSelect(i)}
              onMouseEnter={() => onHover(i)}
              onMouseLeave={() => onHover(undefined)}
              aria-current={i === cursor ? "step" : undefined}
            >
              <span className="n">{i + 1}</span>
              <span className="swatch" style={{ background: s.lane !== undefined ? color[s.lane] : "var(--muted)" }} />
              <span className="text">{s.log}</span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}
