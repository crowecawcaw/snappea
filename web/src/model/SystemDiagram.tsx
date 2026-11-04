import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ElkExtendedEdge, ElkLabel, ElkNode } from "elkjs/lib/elk-api";
import type { ModelInfo, SourceLocation } from "./types";
import { formatPayload, type Trace } from "../trace/model";
import { framesAt } from "../trace/explain";
import { activityAt } from "./activity";
import { replayStates, systemGraph, type SystemGraph } from "./systemGraph";

export function SystemDiagram({ model, trace, cursor, onOpen, readOnly = false, replayControls }: {
  model: ModelInfo; trace?: Trace; cursor?: number; onOpen: (source: SourceLocation) => void; readOnly?: boolean; replayControls?: React.ReactNode;
}) {
  const data = useMemo(() => systemGraph(model, trace), [model, trace]);
  const [layout, setLayout] = useState<{ input: SystemGraph; graph?: ElkNode; error?: string }>();
  const [zoom, setZoom] = useState(1), [expanded, setExpanded] = useState(false);
  const viewport = useRef<HTMLDivElement>(null);
  const marker = useId().replace(/:/g, "");
  const replay = useMemo(() => replayStates(trace, cursor), [trace, cursor]);
  useEffect(() => {
    const worker = new Worker(new URL("./layout.worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<{ graph?: ElkNode; error?: string }>) => setLayout({ input: data, ...event.data });
    worker.onerror = event => setLayout({ input: data, error: event.message || "Layout worker failed" });
    worker.postMessage({ graph: data.graph });
    return () => worker.terminate();
  }, [data]);
  const frames = useMemo(() => trace ? framesAt(trace, cursor ?? -1) : new Map(), [trace, cursor]);
  const activity = activityAt(trace, cursor);
  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const ready = layout?.input === data;
  const width = layout?.graph?.width ?? 640, height = layout?.graph?.height ?? 300;
  const open = (id: string) => { if (!readOnly && data.sources[id]) { setExpanded(false); onOpen(data.sources[id]); } };
  const keys = (id: string) => ({ role: "button", tabIndex: readOnly ? -1 : 0, "aria-disabled": readOnly, onClick: (event: React.MouseEvent) => { event.stopPropagation(); open(id); }, onKeyDown: (event: React.KeyboardEvent) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); open(id); } } });
  const drawLabels = (labels: ElkLabel[] = [], edge = false) => labels.map((l, i) => <g key={i} transform={`translate(${l.x ?? 0} ${l.y ?? 0})`}>
    {edge && <rect className="system-label-bg" width={l.width} height={l.height} rx="3" />}
    <text className={edge ? "system-edge-label" : "system-machine-label"} x={(l.width ?? 0) / 2} y="17" textAnchor="middle">{l.text?.split("\n").map((line, j) => <tspan key={j} x={(l.width ?? 0) / 2} dy={j ? 16 : 0}>{line}</tspan>)}</text>
  </g>);
  const drawEdge = (e: ElkExtendedEdge) => {
    const meta = data.edges[e.id];
    const route = meta?.kind === "route";
    const interactive = !!data.sources[e.id];
    const current = activity?.message && meta.from === activity.fromId && meta.to === activity.toId;
    return <g key={e.id} className={`system-edge ${route ? "route" : "transition"} ${current ? "current" : ""}`} {...(interactive ? keys(e.id) : {})} aria-label={e.labels?.map(l => l.text).join(" ")}>
      {e.sections?.map((section, i) => { const points = [section.startPoint, ...section.bendPoints ?? [], section.endPoint]; return <path key={i} d={points.map((p, j) => `${j ? "L" : "M"}${p.x},${p.y}`).join(" ")} fill="none" stroke={route ? "var(--accent)" : "var(--muted)"} strokeWidth="1.6" strokeDasharray={route ? meta.observed ? undefined : "6 4" : meta?.conditional ? "4 3" : undefined} markerEnd={`url(#${marker}-${route ? "route" : "state"})`} />; })}
      {drawLabels(e.labels, true)}
    </g>;
  };
  const drawNode = (n: ElkNode): React.ReactNode => {
    const meta = data.nodes[n.id]; if (!meta) return null;
    const key = JSON.stringify([meta.machine, meta.state]);
    const frame = meta.lane === undefined ? undefined : frames.get(meta.lane);
    const active = meta.kind === "state" && (trace ? frame?.state === meta.state : replay.active.has(key));
    const failed = active && (trace ? trace.steps[cursor ?? -1]?.kind === "error" && trace.steps[cursor ?? -1]?.lane === meta.lane : replay.failed === key);
    const working = meta.kind === "machine" && meta.lane !== undefined && activity?.step.lane === meta.lane;
    const inbox = (frame?.inbox ?? []) as number[];
    const snapshot = frame?.snapshot;
    const variables = snapshot ? Object.entries({ ...snapshot.fields, ...snapshot.locals }) : [];
    const write = activity && activity.step.lane === meta.lane ? activity.step.inspection?.write : undefined;
    return <g key={n.id} transform={`translate(${n.x ?? 0} ${n.y ?? 0})`} className={`system-node ${meta.kind} ${active ? "active" : ""} ${failed ? "failed" : ""} ${working ? "working" : ""}`} {...(meta.kind === "state" ? keys(n.id) : {})} aria-label={meta.kind === "state" ? `Open state ${meta.label}` : undefined}>
      <rect width={n.width} height={n.height} rx={meta.kind === "machine" ? 14 : 10} />
      {meta.kind === "machine" ? <g className="system-title" {...keys(n.id)} aria-label={`Open machine ${meta.label}`}>{drawLabels(n.labels)}</g> : <g className="system-title"><text className="system-state-label" x={(n.width ?? 0) / 2} y={meta.start ? 28 : 38} textAnchor="middle">{meta.label}</text>{meta.start && <text className="system-start-label" x={(n.width ?? 0) / 2} y="47" textAnchor="middle">start</text>}</g>}
      {n.edges?.map(drawEdge)}{n.children?.map(drawNode)}
      {meta.kind === "machine" && trace && <g className="system-live-data" transform={`translate(16 ${(n.height ?? 200) - 108})`}>
        <text y="14">Inbox · {inbox.length} queued</text>
        <text y="34"><title>{inbox.map(i => `${trace.steps[i].label} ${formatPayload(trace.steps[i].payload)}`).join("\n")}</title>{inbox.length ? `${trace.steps[inbox[0]].label} ${formatPayload(trace.steps[inbox[0]].payload)}`.slice(0, 36) : "No pending messages"}</text>
        <text y="58" className={write ? "system-write" : ""}><title>{write ? `${write.name}: ${write.before} → ${write.after}` : variables.map(([k,v]) => `${k} = ${v}`).join("\n")}</title>{(write ? `${write.name}: ${write.before} → ${write.after}` : variables.slice(0, 2).map(([k,v]) => `${k}=${v}`).join(" · ")).slice(0, 36)}</text>
        <text y="80" className="system-capture">{snapshot ? `Values captured at step ${frame.at + 1}` : frame ? "No values captured yet" : "Not created yet"}</text>
      </g>}
    </g>;
  };
  const motion = (() => {
    if (!ready || !layout?.graph || !activity?.message) return null;
    const edge = layout.graph.edges?.find(e => data.edges[e.id]?.from === activity.fromId && data.edges[e.id]?.to === activity.toId);
    const section = edge?.sections?.[0];
    if (!section) return null;
    const points = [section.startPoint, ...section.bendPoints ?? [], section.endPoint];
    if (activity.delivery) {
      const target = layout.graph.children?.find(n => data.nodes[n.id]?.lane === activity.to);
      const state = target?.children?.find(n => data.nodes[n.id]?.state === frames.get(activity.to)?.state);
      if (target && state) points.push({ x: (target.x ?? 0) + (state.x ?? 0) + (state.width ?? 0) / 2, y: (target.y ?? 0) + (state.y ?? 0) + (state.height ?? 0) / 2 });
    }
    const path = points.map((p, i) => `${i ? "L" : "M"}${p.x},${p.y}`).join(" ");
    const end = points[points.length - 1];
    const text = `${activity.event}${activity.payload === undefined ? "" : ` · ${formatPayload(activity.payload)}`}`;
    const badgeWidth = Math.min(300, Math.max(100, text.length * 7 + 24));
    return <g key={cursor} className="system-message" aria-label={`${activity.delivery ? "Handling" : "Sending"} ${text}`} transform={reducedMotion ? `translate(${end.x} ${end.y})` : undefined}>
      {!reducedMotion && <animateMotion path={path} dur="0.85s" fill="freeze" keyPoints={activity.delivery ? "0.8;1" : "0;1"} keyTimes="0;1" calcMode="linear" />}
      <rect x={-badgeWidth / 2} y="-18" width={badgeWidth} height="36" rx="10" />
      <text textAnchor="middle" y="5">{text.slice(0, 38)}</text><title>{text}</title>
    </g>;
  })();
  return <div className={`system-diagram ${expanded ? "expanded" : ""}`} onKeyDown={e => { if (e.key === "Escape") setExpanded(false); }}>
    <div className="model-toolbar"><strong>All machines</strong><div className="system-zoom"><button aria-label="Zoom out system map" onClick={() => setZoom(z => Math.max(.25, z - .25))}>−</button><button onClick={() => setZoom(Math.min((viewport.current?.clientWidth ?? width) / width, (viewport.current?.clientHeight ?? height) / height))}>Fit</button><button aria-label="Zoom in system map" onClick={() => setZoom(z => Math.min(4, z + .25))}>+</button><button onClick={() => setExpanded(x => !x)}>{expanded ? "Close expanded diagram" : "Expand"}</button></div></div>
    {expanded && replayControls}
    <details className="diagram-help"><summary>Diagram key</summary>
    <div className="system-legend" aria-label="Diagram key"><span><i className="legend-state" />State changes</span><span><i className={trace ? "legend-route observed" : "legend-route"} />{trace ? "Observed event routes" : "Compatible event routes"}</span></div>
    <p className="model-hint">{trace ? "Routes cover the recorded execution. Each box is an actor. Sends enter its inbox; deliveries enter its current state." : "Run Check to see actual routes. Select a machine or state to open its code."}</p>
    </details>
    {!ready && <p role="status">Arranging state machines…</p>}
    {ready && layout.error && <p role="alert">Could not arrange this diagram. {layout.error}</p>}
    {ready && layout.graph && <div className="system-viewport" ref={viewport}><svg className={`system-svg ${activity?.message ? "has-message" : ""}`} aria-label="Linked state-machine diagram" viewBox={`0 0 ${width} ${height}`} style={{ width: zoom * width }}>
      <defs>{["state", "route"].map(kind => <marker key={kind} id={`${marker}-${kind}`} markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0 0 L8 4 L0 8 Z" fill={kind === "route" ? "var(--accent)" : "var(--muted)"} /></marker>)}</defs>
      {layout.graph.children?.map(drawNode)}{layout.graph.edges?.map(drawEdge)}{motion}
    </svg></div>}
    {activity && <div className="system-activity" aria-live="polite"><strong>{activity.delivery ? "Handling" : activity.step.kind === "send" ? "Sending" : activity.step.inspection?.write ? "Data changed" : "Executing"} · {activity.fromId ?? activity.toId ?? "model"}{activity.toId && activity.fromId !== activity.toId ? ` → ${activity.toId}` : ""}</strong>
      <code>{activity.step.inspection?.write ? `${activity.step.inspection.write.name}: ${activity.step.inspection.write.before} → ${activity.step.inspection.write.after}` : activity.event}</code>
      {activity.payload !== undefined && <pre>{typeof activity.payload === "string" ? activity.payload : JSON.stringify(activity.payload, null, 2)}</pre>}
    </div>}
  </div>;
}
