import { useId, useState } from "react";
import type { ModelInfo, ModelMachine, SourceLocation } from "./types";

export function StateDiagram({ model, activeMachine, activeState, onOpen, readOnly = false }: {
  readOnly?: boolean;
  model: ModelInfo; activeMachine?: string; activeState?: string;
  onOpen: (source: SourceLocation) => void;
}) {
  const [chosen, setChosen] = useState("");
  const machine = model.machines.find(m => m.name === (chosen || activeMachine)) ?? model.machines[0];
  if (!machine) return <p className="model-empty">No state machines in this project yet. Add a machine in the editor.</p>;
  return <div className="state-explorer">
    <div className="model-toolbar">
      <label>State machine <select aria-label="State machine" value={machine.name} onChange={e => setChosen(e.target.value)}>
        {model.machines.map(m => <option key={m.name} value={m.name}>{m.name}{m.kind !== "machine" ? ` · ${m.kind}` : ""}</option>)}
      </select></label>
      {activeMachine && <button className="link" onClick={() => setChosen("")}>Follow active actor</button>}
    </div>
    <p className="model-hint">From your code. Arrows show declared or possible <code>goto</code> transitions; conditions still apply. Select a state or action to open its source.</p>
    <MachineGraph readOnly={readOnly} machine={machine} current={machine.name === activeMachine ? activeState : undefined} onOpen={onOpen} />
    <div className="state-actions">
      {machine.states.map(state => <section key={state.name} className={machine.name === activeMachine && state.name === activeState ? "state-action active" : "state-action"}>
        <button className="link" disabled={readOnly} onClick={() => onOpen(state.source)}><strong>{state.name}</strong></button>
        {state.start && <span className="model-tag">start</span>}{state.hot && <span className="model-tag">hot · progress required</span>}
        {state.actions.length === 0 && <p className="muted">No actions declared.</p>}
        {state.actions.map((a, i) => <button className="state-action-row" disabled={readOnly} key={i} onClick={() => onOpen(a.source)}>
          <span>{a.label}</span><span>{a.targets.length ? a.targets.map(t => `${t.conditional ? "may go to" : "→"} ${t.state}`).join(" · ") : "action / stays unless it transitions"}</span>
        </button>)}
      </section>)}
    </div>
  </div>;
}

function MachineGraph({ machine, current, onOpen, readOnly }: { readOnly?: boolean; machine: ModelMachine; current?: string; onOpen: (s: SourceLocation) => void }) {
  const marker = useId().replace(/:/g, "");
  const nodes = machine.states;
  const edges = nodes.flatMap((s, from) => s.actions.flatMap(a => a.targets.map(t => ({ from, to: nodes.findIndex(n => n.name === t.state), label: a.label, ...t })))).filter(e => e.to >= 0);
  const h = Math.max(150, nodes.length * 95 + 45);
  return <div className="state-graph"><svg viewBox={`0 0 540 ${h}`} role="img" aria-label={`States and transitions of ${machine.name}`}>
    <defs><marker id={marker} markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="currentColor" /></marker></defs>
    {edges.map((e, i) => {
      const y1 = 52 + e.from * 95, y2 = 52 + e.to * 95;
      const x = 355 + (i % 7) * 20;
      const d = e.from === e.to ? `M310 ${y1 - 9} C${x + 90} ${y1 - 65},${x + 90} ${y1 + 65},310 ${y1 + 9}` : `M310 ${y1} C${x} ${y1},${x} ${y2},310 ${y2}`;
      return <g key={i}><path d={d} fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray={e.conditional ? "5 4" : undefined} markerEnd={`url(#${marker})`} /><text x={x - 12} y={(y1 + y2) / 2 - 5} className="edge-number">{i + 1}</text><title>{e.label} → {e.state}{e.conditional ? " (conditional)" : ""}</title></g>;
    })}
    {nodes.map((s, i) => <g key={s.name} className={`state-node ${s.name === current ? "active" : ""}`} role="button" aria-disabled={readOnly} tabIndex={readOnly ? -1 : 0} aria-label={`Open state ${s.name}`} onClick={() => onOpen(s.source)} onKeyDown={e => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(s.source); } }}>
      {s.start && <><path d={`M30 ${52 + i * 95} H105`} stroke="currentColor" markerEnd={`url(#${marker})`} /><text x="30" y={42 + i * 95}>start</text></>}
      <rect x="110" y={30 + i * 95} width="200" height="44" rx="22" />
      <text x="210" y={57 + i * 95} textAnchor="middle">{s.name.length > 23 ? s.name.slice(0, 21) + "…" : s.name}</text><title>{s.name}{s.name === current ? " · active in replay" : ""}</title>
    </g>)}
  </svg>
  {edges.length > 0 && <ol className="edge-key">{edges.map((e, i) => <li key={i}><button className="link" disabled={readOnly} onClick={() => onOpen(e.source)}>{nodes[e.from].name} · {e.label} → {e.state}{e.conditional ? " (conditional)" : ""}</button></li>)}</ol>}
  {edges.length === 0 && <p className="model-hint">This machine has no explicit state transitions. Its actions can still send messages and change variables.</p>}
  </div>;
}
