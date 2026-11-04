import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ElkExtendedEdge, ElkLabel, ElkNode } from "elkjs/lib/elk-api";
import type { ModelInfo, SourceLocation } from "./types";
import type { Trace } from "../trace/model";
import { replayStates, systemGraph, type SystemGraph } from "./systemGraph";

export function SystemDiagram({ model, trace, cursor, onOpen, readOnly = false }: {
  model: ModelInfo; trace?: Trace; cursor?: number; onOpen: (source: SourceLocation) => void; readOnly?: boolean;
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
    return <g key={e.id} className={`system-edge ${route ? "route" : "transition"}`} {...(interactive ? keys(e.id) : {})} aria-label={e.labels?.map(l => l.text).join(" ")}>
      {e.sections?.map((section, i) => { const points = [section.startPoint, ...section.bendPoints ?? [], section.endPoint]; return <path key={i} d={points.map((p, j) => `${j ? "L" : "M"}${p.x},${p.y}`).join(" ")} fill="none" stroke={route ? "var(--accent)" : "var(--muted)"} strokeWidth="1.6" strokeDasharray={route ? meta.observed ? undefined : "6 4" : meta?.conditional ? "4 3" : undefined} markerEnd={`url(#${marker}-${route ? "route" : "state"})`} />; })}
      {drawLabels(e.labels, true)}
    </g>;
  };
  const drawNode = (n: ElkNode): React.ReactNode => {
    const meta = data.nodes[n.id]; if (!meta) return null;
    const key = JSON.stringify([meta.machine, meta.state]);
    const active = meta.kind === "state" && replay.active.has(key), failed = active && replay.failed === key;
    return <g key={n.id} transform={`translate(${n.x ?? 0} ${n.y ?? 0})`} className={`system-node ${meta.kind} ${active ? "active" : ""} ${failed ? "failed" : ""}`} {...(meta.kind === "state" ? keys(n.id) : {})} aria-label={meta.kind === "state" ? `Open state ${meta.label}` : undefined}>
      <rect width={n.width} height={n.height} rx={meta.kind === "machine" ? 14 : 10} />
      {meta.kind === "machine" ? <g className="system-title" {...keys(n.id)} aria-label={`Open machine ${meta.label}`}>{drawLabels(n.labels)}</g> : <g className="system-title"><text className="system-state-label" x={(n.width ?? 0) / 2} y={meta.start ? 28 : 38} textAnchor="middle">{meta.label}</text>{meta.start && <text className="system-start-label" x={(n.width ?? 0) / 2} y="47" textAnchor="middle">start</text>}</g>}
      {n.edges?.map(drawEdge)}{n.children?.map(drawNode)}
    </g>;
  };
  return <div className={`system-diagram ${expanded ? "expanded" : ""}`} onKeyDown={e => { if (e.key === "Escape") setExpanded(false); }}>
    <div className="model-toolbar"><strong>All machines</strong><div className="system-zoom"><button aria-label="Zoom out system map" onClick={() => setZoom(z => Math.max(.25, z - .25))}>−</button><button onClick={() => setZoom((viewport.current?.clientWidth ?? width) / width)}>Fit</button><button aria-label="Zoom in system map" onClick={() => setZoom(z => Math.min(4, z + .25))}>+</button><button onClick={() => setExpanded(x => !x)}>{expanded ? "Close expanded diagram" : "Expand"}</button></div></div>
    <div className="system-legend" aria-label="Diagram key"><span><i className="legend-state" />State changes</span><span><i className={trace ? "legend-route observed" : "legend-route"} />{trace ? "Observed event routes" : "Compatible event routes"}</span></div>
    <p className="model-hint">{trace ? "Routes cover the recorded execution. Highlighted states reflect this step; instances are grouped by type." : "Run Check to see actual routes. Select a machine or state to open its code."}</p>
    {!ready && <p role="status">Arranging state machines…</p>}
    {ready && layout.error && <p role="alert">Could not arrange this diagram. {layout.error}</p>}
    {ready && layout.graph && <div className="system-viewport" ref={viewport}><svg className="system-svg" aria-label="Linked state-machine diagram" viewBox={`0 0 ${width} ${height}`} style={{ width: zoom * width }}>
      <defs>{["state", "route"].map(kind => <marker key={kind} id={`${marker}-${kind}`} markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0 0 L8 4 L0 8 Z" fill={kind === "route" ? "var(--accent)" : "var(--muted)"} /></marker>)}</defs>
      {layout.graph.children?.map(drawNode)}{layout.graph.edges?.map(drawEdge)}
    </svg></div>}
    <details className="model-reference"><summary>Diagram data · ELK JSON</summary><pre className="replay-code">{JSON.stringify(data.graph, null, 2)}</pre></details>
  </div>;
}
