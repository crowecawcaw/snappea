import type { Inspection, Step, Trace } from "./model";

export function explainStep(trace: Trace, step: Step): string {
  const who = step.lane === undefined ? "The model" : trace.lanes[step.lane].label;
  const target = step.to === undefined ? "another actor" : trace.lanes[step.to].label;
  const value = step.payload === undefined ? "" : ` with ${JSON.stringify(step.payload)}`;
  if (step.inspection?.write) {
    const w = step.inspection.write;
    return `${who} writes ${w.name}: ${w.before} → ${w.after}.${w.unchanged ? " The value stays the same." : ""}`;
  }
  if (step.inspection) return `${who}: ${step.inspection.phase === "after" ? "assignment completed" : "about to execute this statement"}.`;
  switch (step.kind) {
    case "create": return `${step.lane === step.to ? `${target} is created` : `${who} creates ${target}`}. Each actor has its own state and data.`;
    case "send": return `${who} sends ${step.label}${value} to ${target}. Sending queues the message; it does not execute the receiver's handler.`;
    case "dequeue": return `${who} takes ${step.label}${value} from its inbox and begins or resumes handling it.`;
    case "state": return `${who} enters ${step.label}.${step.hot ? " This is a hot state: the model requires progress out of it." : ""}`;
    case "monitor": return `${who} observes ${step.label}${value} to check a rule. An observer is separate from the actors implementing the system.`;
    case "error": return `This execution violates a rule or encounters a runtime failure. Inspect the preceding messages and assignments to understand how it reached this point.`;
    case "raise": return `${who} raises an event for itself, interrupting its current action.`;
    case "halt": return `${who} stops.`;
    case "print": return `The model prints a message: ${step.log}`;
    default: return step.log;
  }
}

export interface ActorFrame { state?: string; snapshot?: Inspection; at?: number; inbox: number[] }
/** Reconstruct only facts present in the trace. Snapshots keep their capture position visible. */
export function framesAt(trace: Trace, cursor: number): Map<number, ActorFrame> {
  const frames = new Map<number, ActorFrame>();
  for (let i = 0; i <= cursor; i++) {
    const s = trace.steps[i];
    const lane = s.kind === "create" ? s.to : s.lane;
    if (lane !== undefined && !frames.has(lane)) frames.set(lane, { inbox: [] });
    if (s.to !== undefined && s.kind === "send") {
      const f = frames.get(s.to) ?? { inbox: [] };
      f.inbox.push(i); frames.set(s.to, f);
    }
    if (s.lane === undefined) continue;
    const f = frames.get(s.lane)!;
    if (s.kind === "state") f.state = s.label;
    if (s.inspection) { f.snapshot = s.inspection; f.at = i; f.state = s.inspection.state; }
    if (s.kind === "dequeue" && s.peer !== undefined) f.inbox = f.inbox.filter(j => j !== s.peer);
  }
  return frames;
}

export function previousSnapshot(trace: Trace, cursor: number): Inspection | undefined {
  const lane = trace.steps[cursor]?.lane;
  for (let i = cursor - 1; i >= 0; i--) if (trace.steps[i].lane === lane && trace.steps[i].inspection) return trace.steps[i].inspection;
}
