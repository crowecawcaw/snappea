import { useMemo } from "react";
import { Icon, type IconName } from "../ui/Icon";
import type { SourceLocation } from "../model/types";
import { activityAt } from "../model/activity";
import { framesAt, explainStep } from "./explain";
import type { Step, Trace } from "./model";

function iconFor(step: Step): IconName {
  return step.kind === "error" ? "failure" : step.inspection?.write ? "code" : ["send", "dequeue", "announce"].includes(step.kind) ? "message" : step.kind === "monitor" ? "rule" : "actor";
}
function title(trace: Trace, step: Step) {
  const who = step.lane === undefined ? "Model" : trace.lanes[step.lane].label;
  if (step.inspection?.write) return `${who} writes ${step.inspection.write.name}`;
  if (step.kind === "error") return "Execution stops here";
  if (step.kind === "send") return `${who} sends ${step.label}`;
  if (step.kind === "dequeue") return `${who} handles ${step.label}`;
  if (step.kind === "state") return `${who} enters ${step.label}`;
  if (step.kind === "create") return `${trace.lanes[step.to ?? step.lane ?? 0]?.label ?? "Actor"} is created`;
  return `${who} · ${step.label}`;
}
export function ExecutionNotebook({ trace, cursor, rows, stale, onSelect, onOpen }: {
  trace: Trace; cursor: number; rows: number[]; stale?: boolean;
  onSelect: (step: number) => void; onOpen: (source: SourceLocation) => void;
}) {
  const frames = useMemo(() => framesAt(trace, cursor), [trace, cursor]);
  const pos = rows.indexOf(cursor);
  const start = Math.max(0, Math.min(pos - 1, rows.length - 3));
  return <div className="execution-notebook">
    <section className="actor-strip" aria-label="Actor states at this step">
      <h3><Icon name="actor" /> Actors at this step</h3>
      <div className="actor-strip-cards">{[...frames].map(([lane, frame]) => <div key={lane} className={`actor-state-card ${trace.steps[cursor]?.lane === lane ? "current" : ""}`}>
        <strong><Icon name={trace.lanes[lane].monitor ? "rule" : "actor"} />{trace.lanes[lane].label}</strong>
        <span>{frame.state ?? "Starting"}</span><small>{frame.inbox.length} queued</small>
      </div>)}</div>
    </section>
    <section className="notebook-events" aria-label="Execution notebook">
      <h3><Icon name="replay" /> Execution notebook <span>{rows.length} steps</span></h3>
      {start > 0 && <button className="notebook-more" onClick={() => onSelect(rows[Math.max(0, start - 3)])}>↑ Earlier events</button>}
      <ol>{rows.slice(start, start + 3).map((i, offset) => {
        const step = trace.steps[i], selected = i === cursor, activity = activityAt(trace, i)!;
        const write = step.inspection?.write;
        const capture = step.inspection?.source ?? trace.steps.slice(0, i).reverse().find(s => s.lane === step.lane && s.inspection)?.inspection?.source;
        return <li key={i} className={`notebook-event ${selected ? "selected" : ""} ${step.kind === "error" ? "failed" : ""}`}>
          <button className="notebook-event-title" aria-current={selected ? "step" : undefined} onClick={() => onSelect(i)}><span className="notebook-number">{start + offset + 1}</span><Icon name={iconFor(step)} /><strong>{title(trace, step)}</strong></button>
          {selected && <div className="notebook-event-body">
            {activity.message && <div className="event-route"><span>{activity.fromId ?? "Sender not recorded"}</span><Icon name="message" /><span>{activity.toId}</span></div>}
            {activity.payload !== undefined && <div className="event-payload"><span>{activity.message ? "Message data" : "Input data"}</span><pre>{typeof activity.payload === "string" ? activity.payload : JSON.stringify(activity.payload, null, 2)}</pre></div>}
            {write && <div className="event-write"><code>{write.name}</code><span>{write.before}</span><b>→</b><strong>{write.after}</strong>{write.unchanged && <small>unchanged</small>}</div>}
            <p>{explainStep(trace, step)}</p>
            <div className="notebook-links">{capture && <button className="link" disabled={stale} onClick={() => onOpen(capture)}><Icon name="code" />{step.inspection ? "Open statement" : "Last captured statement"} · line {capture.line}</button>}
            {step.kind === "dequeue" && step.peer !== undefined && <button className="link" onClick={() => onSelect(step.peer!)}>Go to matching send</button>}</div>
          </div>}
        </li>;
      })}</ol>
      {start + 3 < rows.length && <button className="notebook-more" onClick={() => onSelect(rows[start + 3])}>Later events ↓</button>}
    </section>
  </div>;
}
