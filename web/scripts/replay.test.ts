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

import { systemGraph, replayStates } from "../src/model/systemGraph.ts";
import type { ModelInfo } from "../src/model/types.ts";
const source = { file: "custom.p", line: 1, col: 1, text: "machine" };
const systemModel: ModelInfo = { tests: [], machines: [
  { name: "Sender", kind: "machine", source, variables: [], rules: [], sent: ["Request"], received: [], states: [{ name: "Sending", start: true, hot: false, source, actions: [] }] },
  { name: "Receiver", kind: "machine", source, variables: [], rules: [], sent: [], received: ["Request"], states: [{ name: "Waiting", start: true, hot: false, source, actions: [] }, { name: "Working", start: false, hot: false, source, actions: [] }] },
  { name: "Unused", kind: "machine", source, variables: [], rules: [], sent: [], received: [], states: [] },
] };

test("system graph distinguishes compatible routes from state transitions", () => {
  const generated = systemGraph(systemModel);
  assert.equal(generated.graph.layoutOptions?.["elk.algorithm"], "layered");
  assert.equal(generated.graph.edges?.length, 1);
  assert.deepEqual(generated.edges.route0, { kind: "route", observed: false, from: "Sender", to: "Receiver" });
  assert.equal(generated.sources.m0.file, "custom.p");
});

test("system replay resolves implementation aliases and preserves simultaneous instance states", () => {
  const trace: ReturnType<typeof parseTrace> = { lanes: [{ id: "Sender(1)", label: "Sender", monitor: false }, { id: "AbstractAlias(2)", label: "Receiver A", monitor: false }, { id: "AbstractAlias(3)", label: "Receiver B", monitor: false }], steps: [
    { kind: "send", lane: 0, to: 1, label: "Request", log: "sent" },
    { kind: "note", lane: 1, label: "snapshot", log: "", inspection: { machine: "Receiver", state: "Waiting", phase: "before", fields: {}, locals: {}, source } },
    { kind: "note", lane: 2, label: "snapshot", log: "", inspection: { machine: "Receiver", state: "Working", phase: "before", fields: {}, locals: {}, source } },
  ] };
  const generated = systemGraph(systemModel, trace);
  assert.equal(Object.values(generated.nodes).some(n => n.machine === "Unused"), false);
  assert.equal(generated.graph.edges?.length, 1);
  assert.equal(generated.edges.route0.observed, true);
  assert.equal(generated.graph.children?.length, 3);
  assert.equal(generated.edges.route0.to, "AbstractAlias(2)");
  assert.deepEqual(Object.values(generated.nodes).filter(n => n.kind === "machine").map(n => n.lane), [0,1,2]);
  assert.equal(replayStates(trace, 2).active.size, 2);
  trace.steps.push({ kind: "error", lane: 2, label: "failed", log: "failed" });
  const failed = replayStates(trace, 3);
  assert.equal(failed.active.size, 2);
  assert.equal(failed.failed, JSON.stringify(["Receiver", "Working"]));
});

import { activityAt } from "../src/model/activity.ts";
test("delivery animation keeps the original sender and payload, even without a state transition", () => {
  const trace = parseTrace(json);
  const sent = activityAt(trace, 3)!;
  const received = activityAt(trace, 4)!;
  assert.equal(sent.fromId, "Main(1)");
  assert.equal(received.fromId, sent.fromId);
  assert.equal(received.toId, "Counter(2)");
  assert.equal(received.payload, 1);
  assert.equal(received.delivery, true);
  assert.equal(framesAt(trace, 3).get(received.to!)?.state, framesAt(trace, 4).get(received.to!)?.state);
});
