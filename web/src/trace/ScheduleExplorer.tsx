import { useEffect, useState } from "react";
import type { ScheduleSamples } from "../protocol";
import { TraceView, type Props } from "./TraceView";

export function ScheduleExplorer({ schedules, bug, ...props }: Props & { schedules?: ScheduleSamples; bug: boolean }) {
  const [selected, setSelected] = useState("result");
  useEffect(() => setSelected("result"), [props.json, schedules]);
  const sample = schedules?.samples.find(s => String(s.number) === selected);
  return <div className="schedule-explorer">
    {schedules && <div className="schedule-browser">
      <label>Execution <select aria-label="Execution" value={selected} onChange={e => setSelected(e.target.value)}>
        <option value="result">{bug ? "Failure" : "First recorded execution"}</option>
        {schedules.samples.map(s => <option key={s.number} value={s.number}>#{s.number} · {s.outcome === "completed" ? "Completed, no failure" : s.outcome === "bounded" ? "Step limit reached" : "Failed"} · {s.steps} choices</option>)}
      </select></label>
      <span>{schedules.total.toLocaleString()} schedules explored · {schedules.bugs.toLocaleString()} failed · {schedules.samples.length} early recordings</span>
      <details><summary>How schedules are checked</summary><p>At each scheduling point the checker chooses an enabled actor or a model choice, then checks assertions and progress rules as execution proceeds. Another schedule can deliver the same messages in a different order.</p><p>Replay retains completed or step-limited executions from the first {schedules.limit} schedules, plus the reported failure. Recordings are also limited to 2,000 choices per execution and 4 MB in total. Search prefixes can stop when they reach an already explored state and are not retained here. A run without a failure is evidence within the configured bounds, not a proof that every possible execution is safe.</p></details>
    </div>}
    <TraceView {...props} key={sample?.trace ?? props.json} json={sample?.trace ?? props.json} />
  </div>;
}
