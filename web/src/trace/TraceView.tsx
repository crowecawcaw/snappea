import { useEffect, useMemo, useState } from "react";
import { ExecutionNotebook } from "./ExecutionNotebook";
import { Icon } from "../ui/Icon";
import { SystemDiagram } from "../model/SystemDiagram";
import type { ModelInfo } from "../model/types";
import { visible } from "./filter";
import { parseTrace } from "./model";
import { laneColors, SequenceDiagram } from "./SequenceDiagram";

export interface Props {
  json: string;
  model?: ModelInfo;
  stale?: boolean;
  view: "diagram" | "messages";
  /** Switches between the replay and the sequence diagram; without it no switch is shown. */
  onViewChange?: (view: "diagram" | "messages") => void;
  onOpenLocation: (file: string, line: number, focus?: boolean) => void;
}

const LOC = /([\w./-]+\.p):(\d+):(\d+)/;
/** Bug trace: step-through controls, machine focus, the diagram, and the step list. */
export function TraceView({ json, view, model, stale, onViewChange, onOpenLocation }: Props) {
  const trace = useMemo(() => parseTrace(json), [json]);
  const color = useMemo(() => laneColors(trace.lanes), [trace]);
  const [cursor, setCursor] = useState(trace.steps.length - 1);
  const [mapOpen, setMapOpen] = useState(false);
  const [follow, setFollow] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [focus, setFocus] = useState<ReadonlySet<number>>(new Set());
  const [hover, setHover] = useState<number>();
  const { rows: allRows, cols } = useMemo(() => visible(trace, focus), [trace, focus]);
  // Source snapshots are useful in the story; keep the communication diagram compact.
  const rows = useMemo(() => view === "messages" ? allRows.filter(i => !trace.steps[i].inspection) : view === "diagram" ? allRows.filter(i => !trace.steps[i].inspection || trace.steps[i].inspection?.write) : allRows, [allRows, trace, view]);
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
  const source = step?.inspection?.source ?? trace.steps.slice(0, cursor).reverse().find(s => step?.lane !== undefined && s.lane === step.lane && s.inspection)?.inspection?.source;
  useEffect(() => {
    if (follow && source && !stale) onOpenLocation(source.file, source.line, false);
  }, [follow, source, stale, onOpenLocation]);
  const loc = trace.error ? LOC.exec(trace.error) : null;
  const capturedAssertion = [...trace.steps].reverse().find(s => s.inspection && /^assert\b/.test(s.inspection.source.text.trimStart()))?.inspection?.source;
  const ruleSource = loc ? { file: loc[1], line: Number(loc[2]) } : capturedAssertion;
  const rule = ruleSource ? model?.machines.flatMap(m => m.rules).find(r => r.source.line === ruleSource.line && (r.source.file === ruleSource.file || ruleSource.file.endsWith("/" + r.source.file))) : undefined;
  const controls = (
        <div className="trace-controls">
          <button onClick={() => toPos(0)} title="First step" aria-label="First step" className="jump"><TransportIcon back end /></button>
          <button onClick={() => toPos(pos - 1)} title="Previous step" aria-label="Previous step"><TransportIcon back /></button>
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
          <button onClick={() => toPos(pos + 1)} title="Next step" aria-label="Next step"><TransportIcon /></button>
          <button onClick={() => toPos(lastPos)} title="Last step" aria-label="Last step" className="jump"><TransportIcon end /></button>
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
    <div className="trace">
      {stale && <div className="stale-trace" role="status">Source or test changed. This is the previous execution. Check again to replay your edits; source links are paused.</div>}
      {(trace.error || rule) && <div className={`replay-verdict ${trace.error ? "" : "passed"}`}>
        <span className="verdict-label"><Icon name="rule" /> {trace.error ? rule ? "Rule violated" : "Check failed" : "Rule checked"}</span>
        {rule && <code className="checked-rule">{rule.condition}</code>}
        <strong>{trace.error ? trace.error.replace(/^Assertion Failed:\s*[^\s]+\.p:\d+:\d+\s*/, "") : "No failure in this recording"}</strong>
        {ruleSource && <button className="link" disabled={stale} onClick={() => onOpenLocation(ruleSource.file, ruleSource.line)}>{rule ? "Open assertion" : "Open source"} · {ruleSource.file}:{ruleSource.line}</button>}
      </div>}
      {onViewChange ? <div className="trace-toolbar">
        <div className="view-switch" role="group" aria-label="Execution view">
          <button aria-pressed={view === "diagram"} onClick={() => onViewChange("diagram")} title="Step through the execution with each actor's state"><Icon name="replay" />Replay</button>
          <button aria-pressed={view === "messages"} onClick={() => onViewChange("messages")} title="The same execution as a sequence diagram of messages between actors"><Icon name="message" />Sequence diagram</button>
        </div>
        {controls}
        <label className="follow-source"><input type="checkbox" checked={follow} disabled={stale} onChange={e => setFollow(e.target.checked)} />Follow source</label>
      </div> : <>
      <div className="replay-tools"><div className="replay-section-label"><Icon name={view === "messages" ? "message" : "replay"} />{view === "messages" ? "Sequence diagram" : "Replay execution"}</div><label className="follow-source"><input type="checkbox" checked={follow} disabled={stale} onChange={e => setFollow(e.target.checked)} />Follow source</label></div>
      {controls}
      </>}
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
      {view === "diagram" ? <div className="notebook-workspace">
        <ExecutionNotebook trace={trace} cursor={cursor} rows={rows} stale={stale} onSelect={go} onOpen={s => { if (!stale) onOpenLocation(s.file, s.line); }} />
        {model && <details className="notebook-map" open={mapOpen} onToggle={e => setMapOpen(e.currentTarget.open)}><summary><Icon name="model" /> State machines <span>See this step on the system map</span></summary>
          {mapOpen && <SystemDiagram replayControls={controls} model={model} trace={trace} cursor={cursor} readOnly={stale} onOpen={s => { if (!stale) onOpenLocation(s.file, s.line); }} />}
        </details>}
      </div> : (
        <>
          <div className="trace-main">
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
          </div>
        </>
      )}
    </div>
  );
}

function TransportIcon({ back = false, end = false }: { back?: boolean; end?: boolean }) {
  return <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" style={{ display: "block", margin: "auto", transform: back ? "rotate(180deg)" : undefined }}>
    <path d="M4 3 L11 8 L4 13 Z" fill="currentColor" />{end && <path d="M13 3 V13" stroke="currentColor" strokeWidth="1.5" />}
  </svg>;
}
