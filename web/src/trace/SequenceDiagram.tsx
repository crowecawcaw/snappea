import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { formatPayload, type Step, type Trace } from "./model";

/** Lane width: narrower on phones so a small model fits the screen. */
const LANE_W = 150;
const LANE_W_NARROW = 96;
const ROW_H = 30;
const TOP = 10;
const HEADER_H = 44;
const MIN_ZOOM = 0.35;
/** Height kept clear at the bottom for the floating Key/zoom controls. */
const PILL_H = 56;
const MAX_ZOOM = 2.5;
/** Machine lanes cycle through this many hues (--lane-0 … in styles.css). */
const HUES = 6;

const laneXFor = (W: number) => (i: number) => i * W + W / 2;
const rowY = (i: number) => TOP + i * ROW_H + ROW_H / 2;
const clamp = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

/** Approximate advance of one 12px monospace character. */
const CH = 7.2;

function clip(s: string, px: number) {
  const n = Math.max(4, Math.floor(px / CH));
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

/** A filled arrowhead ending at (x2, y2), pointing away from (x1, y1). */
function head(x1: number, y1: number, x2: number, y2: number, size = 7) {
  const a = Math.atan2(y2 - y1, x2 - x1);
  const p = (da: number) => `${x2 - size * Math.cos(a + da)},${y2 - size * Math.sin(a + da)}`;
  return `M${x2},${y2} L${p(0.42)} L${p(-0.42)} z`;
}

interface Props {
  trace: Trace;
  /** Step indices to draw, in order. */
  rows: number[];
  /** Lane indices to draw, in order. */
  cols: number[];
  /** Lane colors, indexed by lane (stable while filtering). */
  color: string[];
  focus: ReadonlySet<number>;
  cursor: number;
  hover?: number;
  onSelect: (i: number) => void;
  onHover: (i: number | undefined) => void;
  onToggleLane: (lane: number) => void;
}

/** Lane colors: machines cycle through the palette, specs share one color. */
export function laneColors(lanes: Trace["lanes"]): string[] {
  let n = 0;
  return lanes.map((l) => (l.monitor ? "var(--spec)" : `var(--lane-${n++ % HUES})`));
}

/** Where a lane or step is drawn: its position among the visible ones. */
interface Layout {
  col: (lane: number | undefined) => number | undefined;
  row: (step: number | undefined) => number | undefined;
}

/** A message sequence chart of a bug trace: one lane per machine or spec. Zoomable. */
export function SequenceDiagram({ trace, rows, cols, color, focus, cursor, hover, onSelect, onHover, onToggleLane }: Props) {
  const { lanes, steps } = trace;
  const [W] = useState(() => (matchMedia("(max-width: 600px)").matches ? LANE_W_NARROW : LANE_W));
  const laneX = laneXFor(W);
  const width = Math.max(cols.length, 1) * W;
  // Extra room at the bottom so the last rows can scroll clear of the zoom controls.
  const height = TOP * 2 + rows.length * ROW_H + PILL_H + 16;
  const layout = useMemo<Layout>(() => {
    const c = new Map(cols.map((l, i) => [l, i]));
    const r = new Map(rows.map((s, i) => [s, i]));
    return {
      col: (l) => (l === undefined ? undefined : c.get(l)),
      row: (s) => (s === undefined ? undefined : r.get(s)),
    };
  }, [rows, cols]);
  const scroller = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(1);
  const [legend, setLegend] = useState(false);
  // Content point to keep under the screen point after a zoom.
  const anchor = useRef<{ cx: number; cy: number; sx: number; sy: number } | null>(null);

  // Machine ids like "(3)" are only needed to tell apart machines of the same type.
  const dupNames = useMemo(() => {
    const seen = new Map<string, number>();
    for (const l of lanes) seen.set(baseName(l.id), (seen.get(baseName(l.id)) ?? 0) + 1);
    return new Set([...seen].filter(([, n]) => n > 1).map(([k]) => k));
  }, [lanes]);

  // Where each lane's lifeline starts: its creation step (monitors: the top).
  const born = useMemo(() => {
    const b: number[] = lanes.map((l) => (l.monitor ? 0 : -1));
    rows.forEach((i, pos) => {
      const s = steps[i];
      if (s.kind === "create" && s.to !== undefined && b[s.to] < 0) b[s.to] = pos;
      if (s.lane !== undefined && b[s.lane] < 0) b[s.lane] = pos;
    });
    return b.map((v) => Math.max(v, 0));
  }, [lanes, steps, rows]);

  const fitZoom = useCallback(() => {
    const el = scroller.current;
    return el ? clamp((el.clientWidth - 4) / width) : 1;
  }, [width]);

  const zoomRef = useRef(zoom);

  /**
   * Zooms keeping the content under screen point (sx, sy) (relative to the scroller) in
   * place. Several calls can land before React re-renders (fast pinches, wheel bursts),
   * so the current position is taken from the pending anchor when there is one.
   */
  const zoomTo = useCallback((z: number, sx?: number, sy?: number) => {
    const el = scroller.current;
    if (!el) return;
    const cur = zoomRef.current;
    const next = clamp(z);
    const px = sx ?? el.clientWidth / 2;
    const py = sy ?? el.clientHeight / 2;
    const p = anchor.current;
    const left = p ? p.cx * cur - p.sx : el.scrollLeft;
    const top = p ? p.cy * cur + HEADER_H * cur - p.sy : el.scrollTop;
    anchor.current = { cx: (left + px) / cur, cy: (top + py - HEADER_H * cur) / cur, sx: px, sy: py };
    zoomRef.current = next;
    setZoom(next);
  }, []);

  useLayoutEffect(() => {
    const el = scroller.current;
    const a = anchor.current;
    if (!el || !a) return;
    anchor.current = null;
    el.scrollLeft = a.cx * zoom - a.sx;
    el.scrollTop = a.cy * zoom + HEADER_H * zoom - a.sy;
  }, [zoom]);

  // Start at 100% on wide screens. On phones shrink toward fitting the width, but not
  // so far that labels become unreadable; pinch or the buttons do the rest.
  useLayoutEffect(() => {
    // Labels are drawn at 12px at 100%; start there so they stay readable, and let
    // narrow lanes, sideways step-following, pinch and the buttons handle the rest.
    const z = 1;
    zoomRef.current = z;
    setZoom(z);
  }, [trace, fitZoom]);

  // Ctrl/⌘ + wheel (and trackpad pinch, which browsers report the same way).
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const r = el.getBoundingClientRect();
      const f = Math.exp(-Math.max(-50, Math.min(50, e.deltaY)) * 0.006);
      zoomTo(zoomRef.current * f, e.clientX - r.left, e.clientY - r.top);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomTo]);

  // Two-finger pinch. One finger still scrolls natively (touch-action: pan-x pan-y).
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const pinch = useRef<{ d: number; z: number } | null>(null);
  const dist = () => {
    const [a, b] = [...pointers.current.values()];
    return Math.hypot(a.x - b.x, a.y - b.y);
  };
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.pointerType !== "touch") return;
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.current.size === 2) pinch.current = { d: dist(), z: zoomRef.current };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    if (!pointers.current.has(e.pointerId)) return;
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    const p = pinch.current;
    const el = scroller.current;
    if (!p || pointers.current.size !== 2 || !el) return;
    const [a, b] = [...pointers.current.values()];
    const r = el.getBoundingClientRect();
    zoomTo(p.z * (dist() / p.d), (a.x + b.x) / 2 - r.left, (a.y + b.y) / 2 - r.top);
  };
  const onPointerUp = (e: React.PointerEvent) => {
    pointers.current.delete(e.pointerId);
    if (pointers.current.size < 2) pinch.current = null;
  };

  // Keep the cursor row visible, scrolling only the diagram (never the page).
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const pos = layout.row(cursor);
    if (pos === undefined) return;
    const head = HEADER_H * zoomRef.current;
    const top = head + (rowY(pos) - ROW_H) * zoomRef.current;
    const bottom = head + (rowY(pos) + ROW_H) * zoomRef.current;
    if (top < el.scrollTop + head) el.scrollTop = top - head;
    else if (bottom > el.scrollTop + el.clientHeight - PILL_H) el.scrollTop = bottom - el.clientHeight + PILL_H;
    // Sideways too: bring the step's lanes (sender and receiver) into view.
    const st = steps[cursor];
    if (!st) return;
    const involved = [st.lane, st.to, st.peer !== undefined ? steps[st.peer]?.lane : undefined]
      .map(layout.col)
      .filter((l): l is number => l !== undefined);
    if (!involved.length) return;
    const z = zoomRef.current;
    let left = Math.min(...involved) * W * z;
    let right = (Math.max(...involved) + 1) * W * z;
    if (st.kind === "error") {
      // The bug chip is wider than a lane: bring the whole chip into view.
      const chip = errorChip(st.label, laneXFor(W)(involved[0]), width);
      left = (chip.left - 8) * z;
      right = (chip.left + chip.w + 8) * z;
    }
    if (right - left > el.clientWidth) el.scrollLeft = (left + right) / 2 - el.clientWidth / 2;
    else if (left < el.scrollLeft) el.scrollLeft = left;
    else if (right > el.scrollLeft + el.clientWidth) el.scrollLeft = right - el.clientWidth;
  }, [cursor, steps, W, width, layout]);

  return (
    <div className="seq-wrap">
      <div
        className="seq"
        ref={scroller}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <svg
          className="seq-head"
          width={width * zoom}
          height={HEADER_H * zoom}
          viewBox={`0 0 ${width} ${HEADER_H}`}
          role="presentation"
        >
          {cols.map((li, i) => {
            const l = lanes[li];
            return (
              <g
                key={l.id}
                className={focus.has(li) ? "lane focused" : "lane"}
                transform={`translate(${laneX(i)},${HEADER_H / 2})`}
                style={{ "--c": color[li] } as React.CSSProperties}
                onClick={() => onToggleLane(li)}
              >
                <rect
                  x={-W / 2 + 4}
                  y={-15}
                  width={W - 8}
                  height={30}
                  rx={15}
                  className={l.monitor ? "lane-head monitor" : "lane-head"}
                />
                <LaneLabel id={l.id} monitor={l.monitor} w={W} showId={dupNames.has(baseName(l.id))} focused={focus.has(li)} />
              </g>
            );
          })}
        </svg>
        <svg width={width * zoom} height={height * zoom} viewBox={`0 0 ${width} ${height}`} className="seq-body">
          {cols.map((li, i) => (
            <line
              key={lanes[li].id}
              x1={laneX(i)}
              x2={laneX(i)}
              y1={rowY(born[li]) - ROW_H / 2}
              y2={height}
              className={lanes[li].monitor ? "lifeline monitor" : "lifeline"}
              style={{ "--c": color[li] } as React.CSSProperties}
            />
          ))}
          {rows.map((i) => (
            <Row
              key={i}
              i={i}
              s={steps[i]}
              W={W}
              width={width}
              cursor={cursor}
              hover={hover === i}
              color={color}
              layout={layout}
              onSelect={onSelect}
              onHover={onHover}
            />
          ))}
        </svg>
      </div>
      {legend && <Legend onClose={() => setLegend(false)} />}
      <div className="zoom-controls" role="group" aria-label="Zoom">
        <button className="key-toggle" onClick={() => setLegend(!legend)} aria-expanded={legend}>
          Key
        </button>
        <button onClick={() => zoomTo(zoom / 1.25)} aria-label="Zoom out" disabled={zoom <= MIN_ZOOM}>
          −
        </button>
        <button className="zoom-level" onClick={() => zoomTo(zoom === 1 ? fitZoom() : 1)} title="Toggle 100% / fit width">
          {zoom === 1 ? "Fit" : `${Math.round(zoom * 100)}%`}
        </button>
        <button onClick={() => zoomTo(zoom * 1.25)} aria-label="Zoom in" disabled={zoom >= MAX_ZOOM}>
          +
        </button>
      </div>
    </div>
  );
}

function Row({ i, s, W, width, cursor, hover, color, layout, onSelect, onHover }: {
  i: number; s: Step; W: number; width: number; cursor: number; hover: boolean; color: string[];
  layout: Layout; onSelect: (i: number) => void; onHover: (i: number | undefined) => void;
}) {
  const at = laneXFor(W);
  const laneX = (l: number) => at(layout.col(l) ?? 0);
  const y = rowY(layout.row(i) ?? 0);
  const future = i > cursor;
  const selected = i === cursor;
  const linked = s.peer !== undefined && s.peer === cursor;
  const cls = ["row", future && "future", (selected || linked) && "selected", hover && "hover", `k-${s.kind}`]
    .filter(Boolean)
    .join(" ");
  const x = s.lane !== undefined ? laneX(s.lane) : 0;
  const c = s.lane !== undefined ? color[s.lane] : "var(--fg)";
  let body: React.ReactNode = null;

  switch (s.kind) {
    case "create":
      if (s.lane !== undefined && s.to !== undefined && s.lane !== s.to) {
        const x2 = laneX(s.to);
        const end = x2 + (x2 > x ? -6 : 6);
        body = (
          <>
            <line x1={x} y1={y} x2={end} y2={y} className="msg create" />
            <path d={head(x, y, end, y)} className="arrowhead create" />
            <text x={(x + x2) / 2} y={y - 5} textAnchor="middle" className="msg-label muted">new</text>
          </>
        );
      } else if (s.to !== undefined) {
        body = <circle cx={laneX(s.to)} cy={y} r={4} className="dot" style={{ fill: color[s.to] }} />;
      }
      break;
    case "send": {
      if (s.lane === undefined || s.to === undefined) break;
      const x2 = laneX(s.to);
      const peerRow = layout.row(s.peer);
      const y2 = peerRow !== undefined ? rowY(peerRow) : y;
      const dir = x2 >= x ? 1 : -1;
      const self = s.to === s.lane;
      body = self ? (
        <>
          <path d={`M${x},${y} h18 a8,8 0 0 1 8,8 V${y2 - 8} a8,8 0 0 1 -8,8 h-12`} className="msg" fill="none" />
          <path d={head(x + 18, y2, x + 6, y2)} className="arrowhead" />
          <MsgLabel x={x + 32} y={y - 4} anchor="start" event={s.label} payload={s.payload} max={130} />
        </>
      ) : (
        <>
          <line x1={x} y1={y} x2={x2 - dir * 5} y2={y2} className={s.peer === undefined ? "msg pending" : "msg"} />
          <path d={head(x, y, x2 - dir * 5, y2)} className="arrowhead" />
          <MsgLabel x={x + dir * 10} y={y - 6} anchor={dir > 0 ? "start" : "end"} event={s.label} payload={s.payload} max={Math.max(Math.abs(x2 - x) - 20, 90)} />
        </>
      );
      break;
    }
    case "dequeue":
      body = <circle cx={x} cy={y} r={4} className="dot" />;
      break;
    case "state": {
      const w = Math.min(W - 8, 18 + s.label.length * CH);
      const cx = Math.min(Math.max(x, w / 2 + 2), width - w / 2 - 2);
      body = (
        <>
          <rect x={cx - w / 2} y={y - 10} width={w} height={20} rx={10} className={s.hot ? "state hot" : "state"} />
          <text x={cx} y={y} dy="0.35em" textAnchor="middle" className="state-label">{clip(s.label, W - 16)}</text>
        </>
      );
      break;
    }
    case "monitor": {
      const from = s.to !== undefined ? laneX(s.to) : undefined;
      body = (
        <>
          {from !== undefined && <line x1={from} y1={y} x2={x} y2={y} className="observe" />}
          <rect x={x - 5} y={y - 5} width={10} height={10} rx={2} transform={`rotate(45 ${x} ${y})`} className="diamond" />
          <text x={x + 11} y={y - 6} className="msg-label">{clip(s.label, W - 20)}</text>
        </>
      );
      break;
    }
    case "announce":
    case "raise":
    case "halt":
    case "note":
      body = (
        <>
          <rect x={x - 4} y={y - 4} width={8} height={8} rx={2} className="dot" />
          <text x={x + 9} y={y} dy="0.35em" className="note-label">{clip(s.label, W - 20)}</text>
        </>
      );
      break;
    case "print":
      body = (
        <>
          <rect x={6} y={y - 9} width={Math.min(width - 12, 8 + clip(`print: ${s.label}`, width - 20).length * CH)} height={18} rx={4} className="label-bg" />
          <text x={10} y={y} dy="0.35em" className="print-label">
            {clip(`print: ${s.label}`, width - 20)}
          </text>
        </>
      );
      break;
    case "error": {
      const { left: errLeft, w: errW } = errorChip(s.label, s.lane !== undefined ? x : undefined, width);
      body = (
        <>
          <rect x={errLeft} y={y - 12} width={errW} height={24} rx={12} className="error-bar" />
          <text x={errLeft + 12} y={y} dy="0.35em" className="error-label">{shortError(s.label)}</text>
        </>
      );
      break;
    }
  }
  return (
    <g
      className={cls}
      data-row={i}
      onClick={() => onSelect(i)}
      onMouseEnter={() => onHover(i)}
      onMouseLeave={() => onHover(undefined)}
      style={{ "--c": c } as React.CSSProperties}
    >
      <rect x={0} y={y - ROW_H / 2} width={width} height={ROW_H} className="row-hit" />
      {selected && <rect x={0} y={y - ROW_H / 2 + 3} width={3} height={ROW_H - 6} rx={1.5} className="row-cursor" />}
      <title>{s.log}</title>
      {body}
    </g>
  );
}

/** The bug chip: starts at the lane where the bug happened, kept inside the diagram. */
function errorChip(label: string, laneCenter: number | undefined, width: number) {
  const w = Math.min(width - 8, 24 + shortError(label).length * 7.8);
  const left = Math.max(4, Math.min(laneCenter !== undefined ? laneCenter - 16 : 4, width - w - 4));
  return { left, w };
}

/** "Assertion Failed: file.p:1:2 message" -> "✕ Assertion Failed"; the full text is in the banner. */
function shortError(e: string) {
  const m = /^([^:.]+)/.exec(e);
  return `✕ ${(m?.[1] ?? e).trim()}`;
}

/** "Client(3)" -> name "Client" and a muted "(3)". */
const baseName = (id: string) => id.replace(/\(\d+\)$/, "");

function LaneLabel({ id, monitor, w, showId, focused }: { id: string; monitor: boolean; w: number; showId: boolean; focused: boolean }) {
  const m = /^(.*?)(\(\d+\))?$/.exec(id)!;
  const n = showId ? (m[2] ?? "") : "";
  // Shrink long names (down to ~10px) before truncating, so "Participant(2)" and
  // "Participant(3)" stay distinguishable on narrow lanes.
  const room = w - 14;
  const need = (m[1].length + n.length) * CH;
  const scale = Math.max(0.82, Math.min(1, room / need));
  return (
    <text textAnchor="middle" dy="0.35em" className="lane-label" style={scale < 1 ? { fontSize: `${(12 * scale).toFixed(1)}px` } : undefined}>
      <title>{`${monitor ? `spec ${id}` : id} (${focused ? "click to stop focusing" : "click to show only its steps"})`}</title>
      {clip(m[1], room / scale - n.length * CH)}
      {n && <tspan className="lane-id">{n}</tspan>}
    </text>
  );
}

/** Event name plus a muted payload, on a soft background so lines stay visible. */
function MsgLabel({ x, y, anchor, event, payload, max }: {
  x: number; y: number; anchor: "start" | "middle" | "end"; event: string; payload: unknown; max: number;
}) {
  const ev = clip(event, max);
  const room = max - ev.length * CH - 6;
  const pl = payload !== undefined && room > 30 ? clip(formatPayload(payload), room) : "";
  const w = (ev.length + (pl ? pl.length + 1 : 0)) * CH + 8;
  const left = anchor === "middle" ? x - w / 2 : anchor === "end" ? x - w + 4 : x - 4;
  return (
    <>
      <rect x={left} y={y - 11} width={w} height={15} rx={4} className="label-bg" />
      <text x={x} y={y} textAnchor={anchor} className="msg-label">
        <tspan className="ev">{ev}</tspan>
        {pl && <tspan className="payload"> {pl}</tspan>}
      </text>
    </>
  );
}

/** What the marks mean. Text, not just color. */
function Legend({ onClose }: { onClose: () => void }) {
  const row = (icon: React.ReactNode, text: string) => (
    <li>
      <svg width="40" height="18" viewBox="0 0 40 18" aria-hidden="true">
        {icon}
      </svg>
      <span>{text}</span>
    </li>
  );
  return (
    <div className="legend" role="dialog" aria-label="Diagram key">
      <div className="legend-head">
        <strong>Key</strong>
        <button className="ghost" onClick={onClose} aria-label="Close key">
          ✕
        </button>
      </div>
      <ul>
        {row(
          <>
            <line x1="4" y1="9" x2="30" y2="9" className="msg" style={{ "--c": "var(--lane-0)" } as React.CSSProperties} />
            <path d={head(4, 9, 34, 9)} className="arrowhead" style={{ "--c": "var(--lane-0)" } as React.CSSProperties} />
          </>,
          "Message, in the sender's color; it ends where the receiver handles it",
        )}
        {row(
          <>
            <line x1="4" y1="9" x2="30" y2="9" className="msg pending" style={{ "--c": "var(--lane-0)" } as React.CSSProperties} />
            <path d={head(4, 9, 34, 9)} className="arrowhead" style={{ "--c": "var(--lane-0)" } as React.CSSProperties} />
          </>,
          "Sent but never handled",
        )}
        {row(
          <>
            <line x1="4" y1="9" x2="30" y2="9" className="msg create" />
            <path d={head(4, 9, 34, 9)} className="arrowhead create" />
          </>,
          "Created a machine",
        )}
        {row(
          <rect x="4" y="2" width="32" height="14" rx="7" className="state" style={{ "--c": "var(--lane-1)" } as React.CSSProperties} />,
          "Entered a state",
        )}
        {row(<rect x="4" y="2" width="32" height="14" rx="7" className="state hot" />, "Spec in a hot state (must leave it eventually)")}
        {row(
          <>
            <line x1="4" y1="9" x2="26" y2="9" className="observe" />
            <rect x="26" y="4" width="10" height="10" rx="2" transform="rotate(45 31 9)" className="diamond" />
          </>,
          "Spec observed an event",
        )}
        {row(<rect x="4" y="3" width="32" height="12" rx="6" className="error-bar" />, "Where the bug was detected")}
        {row(
          <rect x="4" y="2" width="32" height="14" rx="7" className="lane-head" style={{ "--c": "var(--lane-0)" } as React.CSSProperties} />,
          "Tap a machine's name to show only its steps",
        )}
      </ul>
    </div>
  );
}
