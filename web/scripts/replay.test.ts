import { strict as assert } from "node:assert";
import { test } from "node:test";
import { parseTrace } from "../src/trace/model.ts";
import { framesAt } from "../src/trace/explain.ts";

const json = JSON.stringify([
  { type: "CreateStateMachine", details: { id: "Main(1)" } },
  { type: "CreateStateMachine", details: { id: "Counter(2)", creatorName: "Main(1)" } },
  { type: "StateTransition", details: { id: "Counter(2)", state: "Serving", isEntry: true } },
  { type: "SendEvent", details: { sender: "Main(1)", target: "Counter(2)", event: "Write", payload: 1 } },
  { type: "DequeueEvent", details: { id: "Counter(2)", event: "Write", payload: 1 } },
  { type: "Inspection", details: { id: "Counter(2)", inspection: { state: "Serving", machine: "Counter", phase: "after", fields: { value: "1" }, locals: {}, source: { file: "custom.p", line: 4, col: 1, text: "value = n;" }, write: { name: "value", scope: "fields", before: "1", after: "1", unchanged: true } } } },
]);

test("replay keeps sends queued until their matching receive and captures real source", () => {
  const trace = parseTrace(json);
  const counter = trace.lanes.findIndex(l => l.id === "Counter(2)");
  assert.deepEqual(framesAt(trace, 3).get(counter)?.inbox, [3]);
  assert.deepEqual(framesAt(trace, 4).get(counter)?.inbox, []);
  assert.equal(framesAt(trace, 4).get(counter)?.snapshot, undefined);
  assert.equal(framesAt(trace, 5).get(counter)?.snapshot?.fields.value, "1");
  assert.equal(trace.steps[5].inspection?.source.file, "custom.p");
  assert.equal(trace.steps[5].inspection?.write?.unchanged, true);
  assert.equal(trace.steps[3].peer, 4);
  assert.equal(trace.steps[4].peer, 3);
});

test("snapshots remain labelled by capture step instead of being invented at later steps", () => {
  const trace = parseTrace(json);
  trace.steps.push({ kind: "state", lane: 1, label: "Done", log: "Counter enters Done" });
  const actor = framesAt(trace, 6).get(1)!;
  assert.equal(actor.state, "Done");
  assert.equal(actor.at, 5);
  assert.equal(actor.snapshot?.state, "Serving");
});
