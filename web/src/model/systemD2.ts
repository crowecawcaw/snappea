import type { ModelInfo, SourceLocation } from "./types";
import type { Trace } from "../trace/model";

/** The renderer only sees generated D2: never user-written D2, imports, images, or URLs. */
export function systemD2(model: ModelInfo, trace?: Trace, cursor = -1) {
  const machineForLane = (lane: number) => trace?.steps.find(s => s.lane === lane && s.inspection)?.inspection?.machine ?? trace?.lanes[lane].id.replace(/\(.*$/, "").replace(/^.*\./, "");
  const participating = trace ? new Set(trace.lanes.map((_, lane) => machineForLane(lane))) : undefined;
  const machines = model.machines.filter(m => m.kind === "machine" && (!participating || participating.has(m.name)));
  const sources: SourceLocation[] = [];
  const link = (source: SourceLocation) => `https://snappea.invalid/model-source/${sources.push(source) - 1}`;
  const q = JSON.stringify;
  const ids = new Map(machines.map((m, i) => [m.name, `m${i}`]));
  const lines = ["direction: right", "vars: {d2-config: {layout-engine: tala}}", "classes: {state: {style: {border-radius: 10; font-size: 15; fill: '#ffffff'; stroke: '#64748b'}}}"];
  const current = new Map<string, Set<string>>();
  const actorStates = new Map<number, string>();
  if (trace) for (const s of trace.steps.slice(0, cursor + 1)) {
    if (s.lane === undefined) continue;
    if (s.kind === "state") actorStates.set(s.lane, s.label);
    if (s.inspection) actorStates.set(s.lane, s.inspection.state);
  }
  for (const [lane, state] of actorStates) {
    const name = machineForLane(lane);
    if (!name) continue;
    const states = current.get(name) ?? new Set(); states.add(state); current.set(name, states);
  }
  const failedStep = trace?.steps[cursor];
  const failedState = failedStep?.kind === "error" && failedStep.lane !== undefined ? actorStates.get(failedStep.lane) : undefined;
  const failedMachine = failedStep?.kind === "error" && failedStep.lane !== undefined ? machineForLane(failedStep.lane) : undefined;
  for (const m of machines) {
    const id = ids.get(m.name)!;
    const instances = trace?.lanes.filter((_, lane) => machineForLane(lane) === m.name).length ?? 0;
    lines.push(`${id}: ${q(m.name + (instances > 1 ? ` × ${instances}` : ""))} {`, "style: {fill: '#f5f7f2'; stroke: '#94a3b8'; font-size: 18; border-radius: 14}", `link: ${q(link(m.source))}`);
    m.states.forEach((s, i) => {
      lines.push(`s${i}: ${q(s.name + (s.start ? "\n(start)" : ""))} {class: state; link: ${q(link(s.source))}${current.get(m.name)?.has(s.name) ? (failedMachine === m.name && failedState === s.name ? "; style.fill: '#fff1eb'; style.stroke: '#c2410c'; style.stroke-width: 3" : "; style.fill: '#dcfce7'; style.stroke: '#166534'; style.stroke-width: 3") : ""}}`);
    });
    for (const [i, state] of m.states.entries()) for (const action of state.actions) for (const t of action.targets) {
      const to = m.states.findIndex(s => s.name === t.state);
      if (to >= 0) lines.push(`s${i} -> s${to}: ${q(action.label + (t.guard ? `\n[${t.guard}]` : ""))} {link: ${q(link(t.source))}; style.stroke: '#475569'; style.font-size: 12${t.conditional ? "; style.stroke-dash: 4" : ""}}`);
    }
    lines.push("}");
  }
  const routes = new Map<string, Set<string>>();
  const route = (from: string, to: string, event: string) => {
    if (!ids.has(from) || !ids.has(to) || from === to) return;
    const key = `${from}\u0000${to}`;
    const events = routes.get(key) ?? new Set(); events.add(event); routes.set(key, events);
  };
  if (trace) {
    for (const s of trace.steps) if (s.kind === "send" && s.lane !== undefined && s.to !== undefined) {
      const from = machineForLane(s.lane), to = machineForLane(s.to);
      if (from && to) route(from, to, s.label);
    }
  } else {
    for (const from of machines) for (const to of machines) for (const event of from.sent ?? []) if (to.received?.includes(event)) route(from.name, to.name, event);
  }
  for (const [key, events] of routes) {
    const [from, to] = key.split("\u0000");
    const labels = [...events];
    lines.push(`${ids.get(from)} -> ${ids.get(to)}: ${q(labels.slice(0, 2).join("\n") + (labels.length > 2 ? `\n+${labels.length - 2} events` : ""))} {style.stroke: '#166534'; style.font-size: 13${trace ? "" : "; style.stroke-dash: 5"}}`);
  }
  return { code: lines.join("\n"), sources, routes: routes.size };
}
