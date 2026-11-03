import { useEffect, useMemo, useRef, useState } from "react";
import type { ModelInfo, SourceLocation } from "./types";
import type { Trace } from "../trace/model";
import { systemD2 } from "./systemD2";

export function TalaDiagram({ model, trace, cursor, onOpen, readOnly = false }: {
  model: ModelInfo; trace?: Trace; cursor?: number; onOpen: (source: SourceLocation) => void; readOnly?: boolean;
}) {
  const graph = useMemo(() => systemD2(model, trace, cursor), [model, trace, cursor]);
  const [rendered, setRendered] = useState<{ code: string; svg?: string; error?: string }>();
  const [zoom, setZoom] = useState(1);
  const [expanded, setExpanded] = useState(false);
  const viewport = useRef<HTMLDivElement>(null);
  const engine = useRef<Promise<import("@d2lang/d2").D2> | undefined>(undefined);
  useEffect(() => () => { const pending = engine.current; engine.current = undefined; void pending?.then(d2 => d2.dispose()).catch(() => {}); }, []);
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        engine.current ??= import("@d2lang/d2").then(({ D2 }) => new D2());
        const d2 = await engine.current;
        if (cancelled) return;
        const compiled = await d2.compile(graph.code, { layout: "tala", pad: 24 });
        const svg = await d2.render(compiled.diagram, { ...compiled.renderOptions, noXMLTag: true });
        if (!cancelled) setRendered({ code: graph.code, svg });
      } catch (error) {
        if (!cancelled) setRendered({ code: graph.code, error: String(error) });
      }
    })();
    return () => { cancelled = true; };
  }, [graph.code]);
  const ready = rendered?.code === graph.code;
  const naturalWidth = Number(/viewBox="[^"]*?\s[^"]*?\s([\d.]+)\s[\d.]+"/.exec(rendered?.svg ?? "")?.[1]) || 640;
  return <div className={`tala-system ${expanded ? "expanded" : ""}`} onKeyDown={e => { if (e.key === "Escape") setExpanded(false); }}>
    <div className="model-toolbar"><strong>All machines</strong><div className="tala-zoom"><button aria-label="Zoom out system map" onClick={() => setZoom(z => Math.max(.5, z - .25))}>−</button><button onClick={() => setZoom((viewport.current?.clientWidth ?? naturalWidth) / naturalWidth)}>Fit</button><button aria-label="Zoom in system map" onClick={() => setZoom(z => Math.min(3, z + .25))}>+</button><button onClick={() => setExpanded(x => !x)}>{expanded ? "Close expanded diagram" : "Expand"}</button></div></div>
    <div className="system-legend" aria-label="Diagram key"><span><i className="legend-state" />State changes</span><span><i className={trace ? "legend-route observed" : "legend-route"} />{trace ? "Observed event routes" : "Compatible event routes"}</span></div>
    <p className="model-hint">{trace ? "Routes cover the recorded execution. Highlighted states reflect this step; instances are grouped by type." : "Run Check to see actual routes. Select a machine or state to open its code."}</p>
    {!ready && <p role="status">Arranging state machines…</p>}
    {ready && rendered.error && <p role="alert">Could not arrange this diagram. {rendered.error}</p>}
    {ready && rendered.svg && <div className="tala-viewport" ref={viewport} onClick={event => {
      const anchor = (event.target as Element).closest("a");
      if (!anchor) return;
      event.preventDefault();
      const href = anchor.getAttribute("href") ?? anchor.getAttribute("xlink:href") ?? "";
      const match = /^https:\/\/snappea\.invalid\/model-source\/(\d+)$/.exec(href);
      if (!readOnly && match && graph.sources[Number(match[1])]) { setExpanded(false); onOpen(graph.sources[Number(match[1])]); }
    }}><div className="tala-svg" style={{ width: `${zoom * naturalWidth}px` }} dangerouslySetInnerHTML={{ __html: rendered.svg }} /></div>}
    <details className="model-reference"><summary>Diagram source · D2 / TALA</summary><pre className="replay-code">{graph.code}</pre></details>
  </div>;
}
