// The README's headline benchmark as a time-lapse race: six P tutorial tests (19,120
// schedules) take 1.8 s with snappea-compat and 189 s with P 3.1.0, 13 s of it compiling.
// Source: the latest entry of upstream.jsonl on the benchmarks branch (2026-11-12).
import { useEffect, useState } from "react";
import { useClock, useInView, useReducedMotion } from "../motion";

const TOTAL = 189;
const SPEEDUP = 40; // the race plays at 40x real time
const RUNS = [
  { name: "snappea", seconds: 1.8, compile: 0 },
  { name: "P 3.1.0", seconds: 189, compile: 13 },
];

export function SpeedBars() {
  const [ref, inView] = useInView<HTMLDivElement>();
  const reduced = useReducedMotion();
  const [startedAt, setStartedAt] = useState<number>();
  const clock = useClock(inView && !reduced);

  // Start the race the first time it's on screen, and again on replay.
  useEffect(() => {
    if (inView && startedAt === undefined) setStartedAt(clock);
  }, [inView, startedAt, clock]);

  const elapsed = reduced ? TOTAL : startedAt === undefined ? 0 : (clock - startedAt) * SPEEDUP;
  const done = elapsed >= TOTAL;

  return (
    <div className="speed" ref={ref}>
      {RUNS.map((r) => {
        const t = Math.min(elapsed, r.seconds);
        const finished = elapsed >= r.seconds;
        return (
          <div key={r.name} className={`speed-row${finished ? " finished" : ""}`}>
            <span className="speed-name">{r.name}</span>
            <span className="speed-track">
              <span className="speed-bar" style={{ width: `max(4px, ${(t / TOTAL) * 100}%)` }}>
                {r.compile > 0 && (
                  <span className="speed-compile" style={{ width: `${(Math.min(t, r.compile) / t || 0) * 100}%` }} />
                )}
              </span>
            </span>
            <span className="speed-time">{finished ? `${r.seconds} s` : `${t.toFixed(t < 1 ? 1 : 0)} s`}</span>
          </div>
        );
      })}
      <div className="speed-foot">
        <span className="speed-legend"><span className="speed-swatch compile" /> compile</span>
        <span className="speed-legend"><span className="speed-swatch check" /> check</span>
        <span className="speed-note">{reduced ? "" : `Played at ${SPEEDUP}× speed.`}</span>
        {done && !reduced && (
          <button className="link" onClick={() => setStartedAt(clock)}>Replay</button>
        )}
      </div>
    </div>
  );
}
