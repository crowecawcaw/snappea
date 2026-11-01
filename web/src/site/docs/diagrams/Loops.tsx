// Diagram 1: a model, a spec and your code. The checker drives the model at design
// time; your tests drive your code at test time; one spec checks the events of both.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useClock, useInView, useInterval, useReducedMotion } from "../../motion";

type Side = "design" | "test";

const SIDES: Record<Side, { label: string; dur: number; gap: number; redEvery: number; bug: string; ok: string; caption: string }> = {
  design: {
    label: "Design time",
    dur: 1.3,
    gap: 0.32,
    redEvery: 9,
    bug: "✗ bug: counter is 1, expected 2",
    ok: "✓ schedule passed",
    caption:
      "You, or an agent, write a model of the design. snappea check runs it thousands of times, each with a different ordering of messages, and the spec flags any run that breaks a rule. None of your service code is involved.",
  },
  test: {
    label: "Test time",
    dur: 2.4,
    gap: 1.1,
    redEvery: 5,
    bug: "✗ test fails: counter is 1, expected 2",
    ok: "✓ events match the spec",
    caption:
      "Your code emits events at the points that matter, from its ordinary unit and integration tests. snappea checks those events against specs written in P, so a rule you checked in the design is also checked in the code.",
  },
};

const NARROW = "(max-width: 700px)";

const PATHS: Record<Side, string> = {
  design: "M120,96 V278 H210 C 240,278 240,170 270,170",
  test: "M600,96 V278 H510 C 480,278 480,170 450,170",
};

export function Loops() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const reduced = useReducedMotion();
  const [side, setSide] = useState<Side>("design");
  const [touched, setTouched] = useState(false);
  // On phones, zoom in on the active side and the spec so the text stays readable.
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const mq = matchMedia(NARROW);
    setNarrow(mq.matches);
    const on = () => setNarrow(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  const t = useClock(inView && !reduced);
  const paths = useRef<Record<Side, SVGPathElement | null>>({ design: null, test: null });
  const [lengths, setLengths] = useState<Record<Side, number>>({ design: 1, test: 1 });

  useLayoutEffect(() => {
    setLengths({
      design: paths.current.design?.getTotalLength() ?? 1,
      test: paths.current.test?.getTotalLength() ?? 1,
    });
  }, []);

  // Alternate sides until the reader picks one.
  useInterval(() => setSide((s) => (s === "design" ? "test" : "design")), 7000, inView && !touched && !reduced);

  const cfg = SIDES[side];
  const dots: { x: number; y: number; red: boolean; key: number }[] = [];
  let lastArrival = -Infinity;
  let lastRed = false;
  const path = paths.current[side];
  if (path && !reduced) {
    const first = Math.max(0, Math.floor((t - cfg.dur) / cfg.gap));
    for (let k = first; k * cfg.gap <= t; k++) {
      const p = (t - k * cfg.gap) / cfg.dur;
      const red = k % cfg.redEvery === cfg.redEvery - 1;
      if (p >= 1) {
        lastArrival = k * cfg.gap + cfg.dur;
        lastRed = red;
        continue;
      }
      const pt = path.getPointAtLength(p * lengths[side]);
      dots.push({ x: pt.x, y: pt.y, red, key: k });
    }
  }
  // Hold a red result on screen for a moment so it can be read.
  const since = t - lastArrival;
  const showBug = !reduced && lastRed && since < 1.6;
  const result = reduced ? null : showBug ? cfg.bug : since < 10 ? cfg.ok : null;

  const pick = (s: Side) => {
    setSide(s);
    setTouched(true);
  };

  const dim = (s: Side) => (side === s ? "" : " dim");

  return (
    <div className="diagram loops" ref={ref}>
      <div className="seg" role="tablist" aria-label="When the spec runs">
        {(Object.keys(SIDES) as Side[]).map((s) => (
          <button key={s} role="tab" aria-selected={side === s} className={side === s ? "on" : ""} onClick={() => pick(s)}>
            {SIDES[s].label}
          </button>
        ))}
      </div>
      <svg className={narrow ? "narrow" : ""} viewBox={narrow ? (side === "design" ? "14 0 462 330" : "244 0 462 330") : "0 0 720 330"} role="img" aria-label="A P model and your code both send events to the same spec.">
        <defs>
          <marker id="loops-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="currentColor" />
          </marker>
        </defs>

        {(["design", "test"] as Side[]).map((s) => (
          <g key={s} className={`edge${dim(s)}`}>
            <path ref={(el) => void (paths.current[s] = el)} d={PATHS[s]} className="motion" />
            <path d={s === "design" ? "M120,96 V244" : "M600,96 V244"} markerEnd="url(#loops-arrow)" />
            <path d={s === "design" ? "M210,278 C 240,278 240,170 268,170" : "M510,278 C 480,278 480,170 452,170"} markerEnd="url(#loops-arrow)" />
            <text x={s === "design" ? 130 : 590} y={176} textAnchor={s === "design" ? "start" : "end"} className="edge-label">
              {s === "design" ? "runs, many orderings" : "calls"}
            </text>
            <text x={s === "design" ? 246 : 474} y={250} textAnchor="middle" className="edge-label">
              events
            </text>
          </g>
        ))}

        {dots.map((d) => (
          <circle key={d.key} cx={d.x} cy={d.y} r={d.red ? 6 : 4.5} className={d.red ? "pulse bug" : "pulse"} />
        ))}

        <Box x={30} y={32} title="snappea check" sub="the model checker" kind="tool" className={dim("design")} />
        <Box x={30} y={246} title="Model" sub="your design, in P" kind="p" className={dim("design")} />
        <Box x={510} y={32} title="Your tests" sub="pytest · JUnit · go test" kind="tool" className={dim("test")} />
        <Box x={510} y={246} title="Your code" sub="the real service" kind="code" className={dim("test")} />
        <Box x={270} y={138} title="Spec" sub="rules that must hold" kind="spec" className={showBug ? "flash" : ""} />

        {result && (
          <text x={360} y={232} textAnchor="middle" className={`result${showBug ? " bad" : " good"}`}>
            {result}
          </text>
        )}
        <text x={120} y={20} textAnchor="middle" className={`side-label${dim("design")}`}>DESIGN TIME</text>
        <text x={600} y={20} textAnchor="middle" className={`side-label${dim("test")}`}>TEST TIME</text>
      </svg>
      <p className="diagram-caption" aria-live="polite">{cfg.caption}</p>
    </div>
  );
}

function Box(props: { x: number; y: number; title: string; sub: string; kind: string; className?: string }) {
  const { x, y, title, sub, kind, className = "" } = props;
  return (
    <g className={`box ${kind} ${className}`}>
      <rect x={x} y={y} width={180} height={64} rx={10} />
      <text x={x + 90} y={y + 28} textAnchor="middle" className="box-title">{title}</text>
      <text x={x + 90} y={y + 48} textAnchor="middle" className="box-sub">{sub}</text>
    </g>
  );
}
