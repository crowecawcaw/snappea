import { strict as assert } from "node:assert";
import { test } from "node:test";
import { D2 } from "@d2lang/d2";
import { systemD2 } from "../src/model/systemD2.ts";
import type { ModelInfo } from "../src/model/types.ts";

// Run the actual D2/TALA compiler: string-only tests cannot catch D2 escaping errors.
test("TALA compiles multiple machines with P nondeterministic choices in transition labels", async () => {
  const source = { file: "choices.p", line: 1, col: 1, text: "if ($$) { goto Done; }" };
  const model: ModelInfo = { tests: [], machines: [
    { name: "Worker", kind: "machine", source, variables: [], rules: [], sent: ["Ready"], received: [], states: [
      { name: "Choosing", start: true, hot: false, source, actions: [{ label: "entry", source, targets: [{ state: "Done", conditional: true, guard: "$$ && value > 0", source }] }] },
      { name: "Done", start: false, hot: false, source, actions: [] },
    ] },
    { name: "Peer", kind: "machine", source, variables: [], rules: [], sent: [], received: ["Ready"], states: [{ name: "Waiting", start: true, hot: false, source, actions: [] }] },
  ] };
  const engine = new D2();
  try {
    const result = await engine.compile(systemD2(model).code, { layout: "tala", pad: 24 });
    assert.ok(result.diagram);
    assert.equal(result.diagram.connections.length, 2);
    const svg = await engine.render(result.diagram, result.renderOptions);
    assert.match(svg, /\$\$/);
    assert.match(svg, /Ready/);
  } finally { await engine.dispose(); }
});
