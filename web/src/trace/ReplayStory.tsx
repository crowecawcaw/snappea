import { useState } from "react";
import type { SourceLocation } from "../model/types";
import { explainStep, framesAt, previousSnapshot } from "./explain";
import type { Trace } from "./model";

export function ReplayStory({ trace, cursor, onSelect, onOpen, sourceAvailable = true }: {
  sourceAvailable?: boolean;
  trace: Trace; cursor: number; onSelect: (i: number) => void; onOpen: (s: SourceLocation) => void;
}) {
  const step = trace.steps[cursor];
  const frames = framesAt(trace, cursor);
  const previous = previousSnapshot(trace, cursor);
  const inspection = step?.inspection;
  const write = inspection?.write;
  if (!step) return <p>No execution steps were recorded.</p>;
  const milestones = [
    { label: "Start", i: 0 },
    { label: "First message", i: trace.steps.findIndex(s => s.kind === "send") },
    { label: "First assignment", i: trace.steps.findIndex(s => s.inspection?.phase === "after") },
    { label: "Failure", i: trace.steps.findIndex(s => s.kind === "error") },
  ].filter(m => m.i >= 0).sort((a, b) => a.i - b.i);
  return <div className="replay-story">
    <details className="replay-landmarks"><summary>Jump to a step</summary><div className="replay-milestones" aria-label="Execution landmarks">{milestones.map(m => <button key={m.label} onClick={() => onSelect(m.i)}>{m.label}<span>step {m.i + 1}</span></button>)}</div></details>
    <section className={`replay-caption ${step.kind === "error" ? "failure" : ""}`} aria-live="polite">
      <span className="eyebrow">STEP {cursor + 1} · {inspection ? inspection.phase === "after" ? "AFTER ASSIGNMENT" : "BEFORE STATEMENT" : step.kind.toUpperCase()}</span>
      <h3>{explainStep(trace, step)}</h3>
      {inspection ? <>
        <button className="link source-link" disabled={!sourceAvailable} onClick={() => onOpen(inspection.source)}>{inspection.source.file}:{inspection.source.line} ↗</button>
        <pre className="replay-code">{inspection.source.text}</pre>
        {inspection.phase === "after" && <div className="assignment-deltas">
          {write ? <div><code>{write.name}</code><span>{write.before} → <strong>{write.after}</strong>{write.unchanged && <em> · same value written again</em>}</span></div> :
            (["fields", "locals"] as const).flatMap(scope => Object.entries(inspection[scope]).filter(([key, val]) => previous?.[scope][key] !== val).map(([key, val]) => <div key={`${scope}-${key}`}><code>{key}</code><span>{previous?.[scope][key] ?? "not captured"} → <strong>{val}</strong></span></div>))}
          <p className="model-hint">The assignment ran even if the stored value stayed the same. Values below were captured by the checker.</p>
        </div>}
      </> : <pre className="replay-code">{step.kind === "error" ? step.label : step.log}</pre>}
      {step.peer !== undefined && <button className="link" onClick={() => onSelect(step.peer!)}>{step.kind === "send" ? "Jump to when this message is handled" : "Jump to when this message was sent"} · step {step.peer + 1}</button>}
    </section>
    <VariableHistory trace={trace} onSelect={onSelect} />
    <details className="replay-actor-details"><summary>Actor variables and pending messages</summary>
    <h3 className="actor-heading">Actors at this point <span>Snapshots and recorded messages</span></h3>
    <div className="replay-actors">{[...frames].map(([lane, frame]) => <section key={lane} className={`replay-actor ${step.lane === lane ? "active" : ""}`}>
      <div className="actor-title"><strong>{trace.lanes[lane].label}</strong><span className="model-tag">{trace.lanes[lane].monitor ? "observer" : "actor"}</span></div>
      <p className="actor-state">{frame.state ?? "Created · waiting to enter its start state"}</p>
      {frame.snapshot ? <>
        <dl className="actor-values">{Object.entries(frame.snapshot.fields).map(([key, val]) => <div key={key}><dt>{key}</dt><dd>{val}</dd></div>)}</dl>
        {Object.keys(frame.snapshot.locals).length > 0 && <details open={step.lane === lane}><summary>Local variables · last captured frame</summary><dl className="actor-values">{Object.entries(frame.snapshot.locals).map(([key, val]) => <div key={key}><dt>{key}</dt><dd>{val}</dd></div>)}</dl></details>}
        <p className="capture-note">Captured {frame.snapshot.phase} step {frame.at! + 1}{frame.at !== cursor ? " · last known values" : ""}</p>
      </> : <p className="capture-note">Variables have not been captured yet.</p>}
      {!trace.lanes[lane].monitor && <div className="actor-inbox"><span className="eyebrow">PENDING MESSAGES · {frame.inbox.length}</span>{frame.inbox.slice(0, 5).map(i => <button key={i} onClick={() => onSelect(i)}>{trace.steps[i].label} <code>{trace.steps[i].payload === undefined ? "" : JSON.stringify(trace.steps[i].payload)}</code></button>)}{frame.inbox.length > 5 && <span>+{frame.inbox.length - 5} more</span>}</div>}
    </section>)}</div>
    </details>
    <p className="model-hint">Replay follows one execution found by the checker. It does not explore new choices as you step. Actor values show their latest capture; inboxes are reconstructed from recorded sends and receives.</p>
  </div>;
}

/** Every write is an observed operation, including writes that do not change the value. */
function VariableHistory({ trace, onSelect }: { trace: Trace; onSelect: (i: number) => void }) {
  const [selected, setSelected] = useState("");
  const histories = new Map<string, { lane: number; name: string; rows: number[]; repeated: number }>();
  trace.steps.forEach((s, i) => {
    const w = s.inspection?.write;
    if (!w || w.scope !== "fields" || s.lane === undefined) return;
    const key = `${s.lane}:${w.name}`;
    const h = histories.get(key) ?? { lane: s.lane, name: w.name, rows: [], repeated: 0 };
    h.rows.push(i); if (w.unchanged) h.repeated++;
    histories.set(key, h);
  });
  const sorted = [...histories].sort((a, b) => b[1].repeated - a[1].repeated || b[1].rows.length - a[1].rows.length);
  const history = histories.get(selected) ?? sorted[0]?.[1];
  if (!history) return null;
  const key = [...histories].find(([, h]) => h === history)![0];
  const messages = trace.steps.map((s, i) => ({ s, i })).filter(({ s }) => (s.kind === "send" || s.kind === "dequeue") && (s.lane === history.lane || s.to === history.lane));
  return <details className="variable-history" open>
    <summary>Investigate this execution · recorded writes</summary>
    <p className="model-hint">Inspect a variable across the entire execution, then jump to a write or a related message. Repeated values are evidence to inspect, not automatically a bug.</p>
    <label>Variable <select aria-label="Variable history" value={key} onChange={e => setSelected(e.target.value)}>{sorted.map(([key, h]) => <option key={key} value={key}>{trace.lanes[h.lane].label}.{h.name} · {h.rows.length} writes</option>)}</select></label>
    <ol className="write-history">{history.rows.map(i => {
      const s = trace.steps[i], w = s.inspection!.write!;
      let received: number | undefined;
      for (let j = i - 1; j >= 0; j--) if (trace.steps[j].lane === s.lane && trace.steps[j].kind === "dequeue") { received = j; break; }
      const sent = received === undefined ? undefined : trace.steps[received].peer;
      const sender = sent === undefined ? undefined : trace.steps[sent].lane;
      let earlierMessage: number | undefined;
      if (sender !== undefined && received !== undefined) {
        for (let j = received - 1; j >= 0; j--) {
          const prior = trace.steps[j];
          if (prior.kind === "send" && prior.lane === history.lane && prior.to === sender) { earlierMessage = j; break; }
        }
      }
      return <li key={i}><button onClick={() => onSelect(i)}><span>Step {i + 1} · {w.name}</span><strong>{w.before} → {w.after}</strong>{w.unchanged && <span className="same-write">same value written again</span>}{received !== undefined && <small>Most recent handled message: {trace.steps[received].label}{sender === undefined ? "" : ` from ${trace.lanes[sender].label}`}</small>}</button>{earlierMessage !== undefined && <button className="prior-message" onClick={() => onSelect(earlierMessage!)}><span>Earlier message to that sender: <strong>{trace.steps[earlierMessage].label}</strong>{trace.steps[earlierMessage].payload === undefined ? "" : ` · ${JSON.stringify(trace.steps[earlierMessage].payload)}`}</span><span>step {earlierMessage + 1} ↗</span></button>}</li>;
    })}</ol>
    <details className="history-messages"><summary>Related messages · {messages.length}</summary><ol>{messages.map(({ s, i }) => <li key={i}><button className="link" onClick={() => onSelect(i)}>Step {i + 1} · {explainStep(trace, s)}</button></li>)}</ol></details>
  </details>;
}
