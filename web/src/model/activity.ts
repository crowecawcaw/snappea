import type { Trace } from "../trace/model";

/** A delivery reuses its matched send: never invent a sender for an unmatched event. */
export function activityAt(trace?: Trace, cursor = -1) {
  const step = trace?.steps[cursor];
  if (!trace || !step) return undefined;
  const send = step.kind === "dequeue" && step.peer !== undefined ? trace.steps[step.peer] : step.kind === "send" ? step : undefined;
  const from = send?.lane ?? (step.kind === "monitor" ? step.to : step.lane);
  const to = send?.to ?? (step.kind === "monitor" || step.kind === "dequeue" ? step.lane : step.to ?? step.lane);
  return { step, from, to, fromId: from === undefined ? undefined : trace.lanes[from].id,
    toId: to === undefined ? undefined : trace.lanes[to].id,
    event: send?.label ?? step.label, payload: step.payload ?? send?.payload,
    delivery: step.kind === "dequeue", message: !!send || step.kind === "monitor" };
}
