import { useId, useState } from "react";
import type { ModelInfo, ModelMachine, SourceLocation } from "./types";

export function StateDiagram({ model, activeMachine, activeState, onOpen, readOnly = false, initialMachine, failed = false }: {
  failed?: boolean;
  initialMachine?: string;
  readOnly?: boolean;
  model: ModelInfo; activeMachine?: string; activeState?: string;
  onOpen: (source: SourceLocation) => void;
}) {
  const [chosen, setChosen] = useState("");
  const machine = model.machines.find(m => m.name === (chosen || activeMachine || initialMachine)) ?? model.machines[0];
  if (!machine) return <p className="model-empty">No state machines in this project yet. Add a machine in the editor.</p>;
  return <div className="state-explorer">
    <div className="model-toolbar">
      {model.machines.length <= 5 ? <div className="machine-picker" role="group" aria-label="State machine">{model.machines.map(m => <button key={m.name} aria-pressed={m.name === machine.name} onClick={() => setChosen(m.name)}>{m.name}</button>)}</div> : <label>State machine <select aria-label="State machine" value={machine.name} onChange={e => setChosen(e.target.value)}>
        {model.machines.map(m => <option key={m.name} value={m.name}>{m.name}{m.kind !== "machine" ? ` · ${m.kind}` : ""}</option>)}
      </select></label>}
      {activeMachine && <button className="link" onClick={() => setChosen("")}>Follow active actor</button>}
    </div>
    <p className="model-hint">States are the boxes. Arrows show how this machine can change state. Select a box or arrow to read its code.</p>
    <MachineGraph failed={failed} readOnly={readOnly} machine={machine} current={machine.name === activeMachine ? activeState : undefined} onOpen={onOpen} />
    <details className="state-actions"><summary>Actions and source</summary>
      {machine.states.map(state => <section key={state.name} className={machine.name === activeMachine && state.name === activeState ? "state-action active" : "state-action"}>
        <button className="link" disabled={readOnly} onClick={() => onOpen(state.source)}><strong>{state.name}</strong></button>
        {state.start && <span className="model-tag">start</span>}{state.hot && <span className="model-tag">hot · progress required</span>}
        {state.actions.length === 0 && <p className="muted">No actions declared.</p>}
        {state.actions.map((a, i) => <button className="state-action-row" disabled={readOnly} key={i} onClick={() => onOpen(a.source)}>
          <span>{a.label}</span><span>{a.targets.length ? a.targets.map(t => `${t.conditional ? "may go to" : "→"} ${t.state}`).join(" · ") : "action / stays unless it transitions"}</span>
        </button>)}
      </section>)}
    </details>
  </div>;
}

function MachineGraph({ machine, current, onOpen, readOnly, failed }: { failed?: boolean; readOnly?: boolean; machine: ModelMachine; current?: string; onOpen: (s: SourceLocation) => void }) {
  const marker = useId().replace(/:/g, "");
  const nodes = machine.states;
  const edges = nodes.flatMap((s, from) => s.actions.flatMap(a => a.targets.map(t => ({ from, to: nodes.findIndex(n => n.name === t.state), label: a.label, ...t })))).filter(e => e.to >= 0);
  const spacing = 145;
  const center = (i: number) => 65 + i * spacing;
  const h = Math.max(180, nodes.length * spacing + 20);
  return <div className="state-graph"><div className="state-graph-title"><strong>{machine.name}</strong><span>{current ? `${failed ? "Failure detected in" : "Current state:"} ${current}` : "Declared states"}</span></div><svg viewBox={`0 0 620 ${h}`} role="img" aria-label={`States and transitions of ${machine.name}`}>
    <defs><marker id={marker} markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="currentColor" /></marker></defs>
    {edges.map((e, i) => {
      const y1 = center(e.from), y2 = center(e.to);
      const x = 355 + (i % 5) * 12;
      const d = e.from === e.to ? `M310 ${y1 - 18} C${x + 70} ${y1 - 85},${x + 70} ${y1 + 85},310 ${y1 + 18}` : `M310 ${y1} C${x} ${y1},${x} ${y2},310 ${y2}`;
      const labelY = e.from === e.to ? y1 : (y1 + y2) / 2;
      return <g key={i} className="state-edge" role="button" aria-label={`${nodes[e.from].name}: ${e.label} to ${e.state}`} aria-disabled={readOnly} tabIndex={readOnly ? -1 : 0} onClick={() => { if (!readOnly) onOpen(e.source); }} onKeyDown={event => { if (!readOnly && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); onOpen(e.source); } }}>
        <path d={d} fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray={e.conditional ? "5 4" : undefined} markerEnd={`url(#${marker})`} />
        <text x={395} y={labelY - 3} className="edge-label">{e.label.length > 28 ? e.label.slice(0, 26) + "…" : e.label}</text>
        {e.conditional && <text x={395} y={labelY + 14} className="edge-condition">{e.guard ? (e.guard.length > 35 ? e.guard.slice(0, 33) + "…" : e.guard) : "when its condition is true"}</text>}
        <title>{e.label} → {e.state}{e.conditional ? " (conditional)" : ""}</title>
      </g>;
    })}
    {nodes.map((s, i) => {
      const actions = s.actions.filter(a => !a.targets.length).map(a => a.label === "entry" ? "on entering" : a.label === "exit" ? "on leaving" : a.label);
      return <g key={s.name} className={`state-node ${s.name === current ? `active ${failed ? "failed" : ""}` : ""}`} role="button" aria-disabled={readOnly} tabIndex={readOnly ? -1 : 0} aria-label={`Open state ${s.name}`} onClick={() => { if (!readOnly) onOpen(s.source); }} onKeyDown={e => { if (!readOnly && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); onOpen(s.source); } }}>
        {s.start && <><path d={`M12 ${center(i)} H78`} stroke="currentColor" markerEnd={`url(#${marker})`} /><text x="12" y={center(i) - 10}>start</text></>}
        <rect x="85" y={center(i) - 42} width="225" height="84" rx="16" />
        <text x="197" y={center(i) - 14} textAnchor="middle">{s.name.length > 25 ? s.name.slice(0, 23) + "…" : s.name}</text>
        <text className="state-node-actions" x="197" y={center(i) + 8} textAnchor="middle">{actions.slice(0, 2).join(" · ").slice(0, 35)}</text>
        {actions.length > 2 && <text className="state-node-actions" x="197" y={center(i) + 25} textAnchor="middle">+ {actions.length - 2} more actions</text>}
        {s.name === current && <text className="state-current" x="197" y={center(i) + 60} textAnchor="middle">{failed ? "● failure detected here" : "● current state"}</text>}
        <title>{s.name}{s.name === current ? " · active in replay" : ""}</title>
      </g>;
    })}
  </svg>
  {edges.length === 0 && <p className="model-hint">One state can handle many events. This machine has no explicit state changes.</p>}
  </div>;
}
