import { strict as assert } from "node:assert";
import { test } from "node:test";
import ELK from "elkjs/lib/elk.bundled.js";
import { systemGraph } from "../src/model/systemGraph.ts";
import type { ModelInfo } from "../src/model/types.ts";

// Exercise the real layout engine with nesting, ports and cross-machine connections.
test("ELK routes nested machines and treats P choice labels as literal text", async () => {
  const source = { file: "choices.p", line: 1, col: 1, text: "if ($$) { goto Done; }" };
  const model: ModelInfo = { tests: [], machines: [
    { name: "Worker", kind: "machine", source, variables: [], rules: [], sent: ["Ready"], received: [], states: [
      { name: "Choosing", start: true, hot: false, source, actions: [{ label: "entry", source, targets: [{ state: "Done", conditional: true, guard: "$$ && value > 0", source }] }] },
      { name: "Done", start: false, hot: false, source, actions: [] },
    ] },
    { name: "Peer", kind: "machine", source, variables: [], rules: [], sent: [], received: ["Ready"], states: [{ name: "Waiting", start: true, hot: false, source, actions: [] }] },
  ] };
  const data = systemGraph(model);
  const layout = await new ELK().layout(data.graph);
  assert.ok(Number.isFinite(layout.width) && layout.width! > 0);
  assert.ok(Number.isFinite(layout.height) && layout.height! > 0);
  assert.equal(layout.children?.length, 2);
  assert.equal(layout.edges?.length, 1);
  assert.ok(layout.edges?.[0].sections?.length);
  const worker = layout.children?.find(n => data.nodes[n.id].machine === "Worker");
  assert.equal(worker?.children?.length, 2);
  assert.match(worker?.edges?.[0].labels?.[0].text ?? "", /\$\$/);
  assert.ok(worker?.edges?.[0].sections?.length);
  const allEdges = [...layout.edges ?? [], ...worker?.edges ?? []];
  for (const edge of allEdges) for (const section of edge.sections ?? []) {
    for (const point of [section.startPoint, ...section.bendPoints ?? [], section.endPoint]) {
      assert.ok(Number.isFinite(point.x) && Number.isFinite(point.y));
    }
  }
});
