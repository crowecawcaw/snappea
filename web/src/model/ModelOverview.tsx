import { Icon } from "../ui/Icon";
import { SystemDiagram } from "./SystemDiagram";
import type { ModelInfo, SourceLocation } from "./types";

export function ModelOverview({ model, testCase, pending, onOpen }: {
  model?: ModelInfo; testCase: string; pending: boolean; onOpen: (s: SourceLocation) => void;
}) {
  if (!model) return <div className="model-overview"><h2><Icon name="model" /> State machines</h2><p>{pending ? "Reading your model…" : "Fix the source errors to update the diagrams."}</p></div>;
  const test = model.tests.find(t => t.name === testCase);
  return <div className="model-overview">
    <div className="model-heading"><h2><Icon name="model" /> State machines</h2><span className="model-hint">From your current code</span></div>
    <SystemDiagram model={model} onOpen={onOpen} />
    <details className="model-reference"><summary>Test and assertions</summary>
      {test && <section className="test-brief"><button className="link" onClick={() => onOpen(test.source)}><strong>{test.name}</strong></button><p>Starts with {test.main}.</p><pre>{test.composition}</pre></section>}
      <div className="model-rules">{model.machines.flatMap(m => m.rules.map((r, i) => <button key={`${m.name}-${i}`} onClick={() => onOpen(r.source)}><span>{m.name}</span><code>{r.condition}</code></button>))}</div>
      <p className="model-hint">The test chooses which declarations participate. Diagrams show control flow in the source; conditions still apply.</p>
    </details>
  </div>;
}
