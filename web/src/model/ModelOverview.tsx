import { useState } from "react";
import type { ModelInfo, SourceLocation } from "./types";
import { StateDiagram } from "./StateDiagram";

export function ModelOverview({ model, testCase, pending, onOpen }: {
  model?: ModelInfo; testCase: string; pending: boolean; onOpen: (s: SourceLocation) => void;
}) {
  const [view, setView] = useState<"overview" | "states">("overview");
  if (!model) return <div className="model-overview"><h2>Explore the model you edit</h2><p>{pending ? "Reading your model…" : "Fix the source errors to update the model overview and state diagrams."}</p><p className="muted">Edit any example or open your own files. Check explores different executions of the selected test.</p></div>;
  const test = model.tests.find(t => t.name === testCase);
  return <div className="model-overview">
    <div className="model-heading"><div><span className="eyebrow">YOUR EDITABLE MODEL</span><h2>What are we checking?</h2></div><div className="view-switch" aria-label="Model view">
      <button aria-pressed={view === "overview"} onClick={() => setView("overview")}>Overview</button><button aria-pressed={view === "states"} onClick={() => setView("states")}>State diagrams</button>
    </div></div>
    {pending && <p role="status" className="model-hint">Updating from your edits…</p>}
    {view === "states" ? <StateDiagram model={model} onOpen={onOpen} /> : <>
      <p className="model-intro">State machines keep their own data and communicate through events. The checker explores different choices and execution orders to find a rule violation.</p>
      {test && <section className="test-brief"><span className="eyebrow">SELECTED TEST</span><button className="link" onClick={() => onOpen(test.source)}><strong>{test.name}</strong></button><p>Starts with <strong>{test.main}</strong>. The test defines which machines and rules participate.</p><details><summary>Test composition</summary><pre>{test.composition}</pre></details></section>}
      <h3>Actors and rules</h3><div className="model-actors">{model.machines.map(m => <button key={m.name} className="model-actor" onClick={() => onOpen(m.source)}><span className="model-tag">{m.kind === "machine" ? "actor" : "rule observer"}</span><strong>{m.name}</strong><span>{m.states.length} state{m.states.length !== 1 ? "s" : ""}{m.variables.length ? ` · ${m.variables.map(v => v.name).join(", ")}` : ""}</span></button>)}</div>
      <p className="model-hint">Declarations in this project; the selected test may use only some of them. Select any actor to read its code.</p>
      <h3>Assertions in the source</h3><div className="model-rules">{model.machines.flatMap(m => m.rules.map((r, i) => <button key={`${m.name}-${i}`} onClick={() => onOpen(r.source)}><span>{m.name}</span><code>{r.condition}</code></button>))}</div>
      {!model.machines.some(m => m.rules.length) && <p className="muted">No assertions found in state actions. Rules may also live in functions; hot states express progress obligations.</p>}
      <div className="model-next"><strong>Edit → Check → Replay</strong><p>Change the source, then press Check. If a bug is found, walk through the recorded execution with variables, messages, and source side by side.</p><p className="muted">This checks your model under its assumptions and search limits. It does not execute your application.</p></div>
    </>}
  </div>;
}
