import { Icon } from "../ui/Icon";
import { useEffect, useState } from "react";
import type { ScheduleSamples } from "../protocol";
import { TraceView, type Props } from "./TraceView";

export function ScheduleExplorer({ schedules, bug, downloads, ...props }: Props & { schedules?: ScheduleSamples; bug: boolean; downloads?: React.ReactNode }) {
  const [selected, setSelected] = useState("result");
  useEffect(() => setSelected("result"), [props.json, schedules]);
  const sample = schedules?.samples.find(s => String(s.number) === selected);
  return <div className="schedule-explorer">
    {schedules && <div className="schedule-browser">
      <label><Icon name="replay" />Execution <select aria-label="Execution" value={selected} onChange={e => setSelected(e.target.value)}>
        <option value="result">{bug ? "Failure" : "First recorded execution"}</option>
        {schedules.samples.filter(s => s.trace !== props.json).map(s => <option key={s.number} value={s.number}>#{s.number} · {s.outcome === "completed" ? "Completed, no failure" : s.outcome === "bounded" ? "Step limit reached" : "Failed"} · {s.steps} choices</option>)}
      </select></label>
      <details className="run-details"><summary>Run details</summary><div className="run-details-body"><strong>{schedules.total.toLocaleString()} schedules explored · {schedules.bugs.toLocaleString()} failed</strong><p>{schedules.samples.length} early recordings retained.</p><p>At each scheduling point the checker chooses an enabled actor or a model choice, then checks assertions and progress rules as execution proceeds. Another schedule can deliver the same messages in a different order.</p><p>Replay retains completed or step-limited executions from the first {schedules.limit} schedules, plus the reported failure. Recordings are also limited to 2,000 choices per execution and 4 MB in total. Search prefixes can stop when they reach an already explored state and are not retained here. A run without a failure is evidence within the configured bounds, not a proof that every possible execution is safe.</p>{downloads}</div></details>
    </div>}
    <TraceView {...props} key={sample?.trace ?? props.json} json={sample?.trace ?? props.json} />
  </div>;
}
