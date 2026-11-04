import type { ElkLabel, ElkNode, ElkExtendedEdge } from "elkjs/lib/elk-api";
import type { ModelInfo, SourceLocation } from "./types";
import type { Trace } from "../trace/model";

export interface DiagramNode { kind: "machine" | "state"; machine: string; state?: string; label: string; lane?: number; start?: boolean }
export interface DiagramEdge { kind: "transition" | "route"; conditional?: boolean; observed?: boolean; from?: string; to?: string }
export interface SystemGraph {
  graph: ElkNode;
  nodes: Record<string, DiagramNode>;
  edges: Record<string, DiagramEdge>;
  sources: Record<string, SourceLocation>;
}

export function laneTypes(trace?: Trace): Map<number, string> {
  const types = new Map<number, string>();
  trace?.lanes.forEach((lane, i) => types.set(i, lane.id.replace(/\(.*$/, "").replace(/^.*\./, "")));
  for (const step of trace?.steps ?? []) if (step.lane !== undefined && step.inspection) types.set(step.lane, step.inspection.machine);
  return types;
}

/** ELK receives structured data. Labels stay literal, including P's `$` expressions. */
export function label(text: string): ElkLabel {
  const lines = text.split("\n").flatMap(line => line.match(/.{1,36}(?:\s|$)|.{1,36}/g)?.map(s => s.trim()) ?? [""]);
  return { text: lines.join("\n"), width: Math.max(20, ...lines.map(s => s.length * 7)) + 12, height: lines.length * 16 + 8 };
}

export function systemGraph(model: ModelInfo, trace?: Trace): SystemGraph {
  const types = laneTypes(trace);
  const machines = trace
    ? trace.lanes.flatMap((lane, i) => { const m = model.machines.find(m => m.name === types.get(i)); return m ? [{ ...m, identity: lane.id, lane: i }] : []; })
    : model.machines.filter(m => m.kind === "machine").map(m => ({ ...m, identity: m.name, lane: undefined as number | undefined }));
  const ids = new Map(machines.map((m, i) => [m.identity, `m${i}`]));
  const nodes: SystemGraph["nodes"] = {}, edges: SystemGraph["edges"] = {}, sources: SystemGraph["sources"] = {};
  const children: ElkNode[] = [];
  for (const m of machines) {
    const id = ids.get(m.identity)!;
    const title = m.identity + (m.kind === "spec" ? " · rule" : "");
    nodes[id] = { kind: "machine", machine: m.name, lane: m.lane, label: title }; sources[id] = m.source;
    const states = m.states.map((s, i) => {
      const stateId = `${id}-s${i}`;
      nodes[stateId] = { kind: "state", machine: m.name, lane: m.lane, state: s.name, label: s.name, start: s.start }; sources[stateId] = s.source;
      return { id: stateId, width: Math.max(trace ? 240 : 140, Math.min(350, s.name.length * 8 + 28)), height: 56 };
    });
    const transitions: ElkExtendedEdge[] = [];
    for (const [i, state] of m.states.entries()) for (const action of state.actions) for (const t of action.targets) {
      const to = m.states.findIndex(s => s.name === t.state);
      if (to < 0) continue;
      const edgeId = `${id}-t${transitions.length}`;
      edges[edgeId] = { kind: "transition", conditional: t.conditional }; sources[edgeId] = t.source;
      transitions.push({ id: edgeId, sources: [`${id}-s${i}`], targets: [`${id}-s${to}`], labels: [label(action.label + (t.guard ? `\n[${t.guard}]` : ""))] });
    }
    children.push({ id, children: states, edges: transitions, labels: [label(title)], ports: [],
      layoutOptions: { "elk.algorithm": "layered", "elk.direction": "DOWN", "elk.padding": trace ? "[top=52,left=24,bottom=136,right=24]" : "[top=52,left=24,bottom=24,right=24]", "elk.spacing.nodeNode": "24", "elk.layered.spacing.nodeNodeBetweenLayers": "40", "elk.nodeLabels.placement": "INSIDE H_CENTER V_TOP", "elk.portConstraints": "FIXED_SIDE" },
    });
  }
  const routes = new Map<string, Set<string>>();
  const route = (from: string | undefined, to: string | undefined, event: string) => {
    if (!from || !to || !ids.has(from) || !ids.has(to)) return;
    const key = `${from}\u0000${to}`, events = routes.get(key) ?? new Set<string>(); events.add(event); routes.set(key, events);
  };
  if (trace) {
    for (const s of trace.steps) if (s.kind === "send" && s.lane !== undefined && s.to !== undefined) route(trace.lanes[s.lane].id, trace.lanes[s.to].id, s.label);
    for (const s of trace.steps) if (s.kind === "monitor" && s.lane !== undefined && s.to !== undefined) route(trace.lanes[s.to].id, trace.lanes[s.lane].id, s.label);
  } else {
    for (const from of machines) for (const to of machines) for (const event of from.sent ?? []) if (to.received?.includes(event)) route(from.name, to.name, event);
  }
  const connections: ElkExtendedEdge[] = [];
  for (const [key, events] of routes) {
    const [from, to] = key.split("\u0000"), fromId = ids.get(from)!, toId = ids.get(to)!;
    const edgeId = `route${connections.length}`, out = `${fromId}-${edgeId}-out`, into = `${toId}-${edgeId}-in`;
    children.find(n => n.id === fromId)!.ports!.push({ id: out, width: 6, height: 6, layoutOptions: { "elk.port.side": "EAST" } });
    children.find(n => n.id === toId)!.ports!.push({ id: into, width: 6, height: 6, layoutOptions: { "elk.port.side": "WEST" } });
    const names = [...events];
    connections.push({ id: edgeId, sources: [out], targets: [into], labels: [label(names.slice(0, 2).join("\n") + (names.length > 2 ? `\n+${names.length - 2} events` : ""))] });
    edges[edgeId] = { kind: "route", observed: !!trace, from, to };
  }
  return { graph: { id: "system", children, edges: connections, layoutOptions: {
    "elk.algorithm": "layered", "elk.direction": "RIGHT", "elk.edgeRouting": "ORTHOGONAL", "elk.hierarchyHandling": "SEPARATE_CHILDREN", "elk.padding": "[top=24,left=24,bottom=24,right=24]", "elk.spacing.nodeNode": "60", "elk.layered.spacing.nodeNodeBetweenLayers": "90", "elk.spacing.edgeNode": "24", "elk.layered.spacing.edgeNodeBetweenLayers": "30",
  } }, nodes, edges, sources };
}

/** Separate replay styling from layout so stepping never triggers another layout. */
export function replayStates(trace?: Trace, cursor = -1) {
  const types = laneTypes(trace), actorStates = new Map<number, string>();
  for (const step of trace?.steps.slice(0, cursor + 1) ?? []) {
    if (step.lane === undefined) continue;
    if (step.kind === "state") actorStates.set(step.lane, step.label);
    if (step.inspection) actorStates.set(step.lane, step.inspection.state);
  }
  const active = new Set<string>();
  for (const [lane, state] of actorStates) active.add(JSON.stringify([types.get(lane), state]));
  const failure = trace?.steps[cursor];
  const failed = failure?.kind === "error" && failure.lane !== undefined ? JSON.stringify([types.get(failure.lane), actorStates.get(failure.lane)]) : undefined;
  return { active, failed };
}
