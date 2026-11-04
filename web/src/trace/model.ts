import type { SourceLocation } from "../model/types";
// Turns the checker's JSON bug trace (upstream PChecker format) into lanes and
// steps for the sequence diagram.

export interface Lane {
  id: string;
  label: string;
  monitor: boolean;
}

export type StepKind =
  | "create"
  | "send"
  | "dequeue"
  | "state"
  | "monitor"
  | "announce"
  | "raise"
  | "print"
  | "halt"
  | "error"
  | "note";

export interface Inspection {
  write?: { scope: "fields" | "locals"; name: string; before: string; after: string; unchanged: boolean } | null;
  source: SourceLocation;
  machine: string;
  state: string;
  phase: "before" | "after";
  fields: Record<string, string>;
  locals: Record<string, string>;
}
export interface Step {
  inspection?: Inspection;
  kind: StepKind;
  /** Lane the step happens on (the actor). */
  lane?: number;
  /** For create/send: the other lane. */
  to?: number;
  /** Short label drawn in the diagram. */
  label: string;
  /** Upstream's log line. */
  log: string;
  payload?: unknown;
  /** Send <-> dequeue pairing (step indices). */
  peer?: number;
  hot?: boolean;
}

export interface Trace {
  lanes: Lane[];
  steps: Step[];
  error?: string;
  strategy?: string;
}

interface Entry {
  type: string;
  details: Record<string, unknown> & { log?: string };
}

const str = (v: unknown) => (typeof v === "string" ? v : undefined);

export function parseTrace(json: string): Trace {
  const entries = JSON.parse(json) as Entry[];
  const lanes: Lane[] = [];
  const laneIndex = new Map<string, number>();
  const steps: Step[] = [];
  const inFlight = new Map<number, { event: string; key: string; step: number }[]>();
  let error: string | undefined;
  let strategy: string | undefined;

  const lane = (id: string | undefined, monitor = false): number | undefined => {
    if (!id) return undefined;
    let i = laneIndex.get(id);
    if (i === undefined) {
      i = lanes.length;
      lanes.push({ id, label: id, monitor });
      laneIndex.set(id, i);
    }
    return i;
  };
  const key = (p: unknown) => JSON.stringify(p ?? null);

  for (const { type, details: d } of entries) {
    const log = d.log ?? type;
    const payload = d.payload === "null" ? undefined : d.payload;
    switch (type) {
      case "Inspection": {
        const inspection = d.inspection as Inspection;
        steps.push({ kind: "note", lane: lane(str(d.id)), label: inspection.source.text.split("\n")[0], log, inspection });
        break;
      }
      case "CreateMonitor":
        lane(str(d.monitor), true);
        break;
      case "CreateStateMachine": {
        const to = lane(str(d.id));
        const from = laneIndex.get(str(d.creatorName) ?? "");
        steps.push({ kind: "create", lane: from ?? to, to, label: "new", log, payload });
        break;
      }
      case "StateTransition":
      case "MonitorStateTransition":
        if (d.isEntry) {
          steps.push({
            kind: "state",
            lane: lane(str(d.id) ?? str(d.monitor), type === "MonitorStateTransition"),
            label: str(d.state) ?? "?",
            log,
            hot: d.isInHotState === true || /enters hot state/.test(log),
          });
        }
        break;
      case "SendEvent": {
        const from = lane(str(d.sender));
        const to = lane(str(d.target));
        const i = steps.length;
        steps.push({ kind: "send", lane: from, to, label: str(d.event) ?? "?", log, payload });
        if (to !== undefined) {
          const q = inFlight.get(to) ?? [];
          q.push({ event: str(d.event) ?? "", key: key(payload), step: i });
          inFlight.set(to, q);
        }
        break;
      }
      case "DequeueEvent":
      case "ReceiveEvent": {
        const at = lane(str(d.id));
        const event = str(d.event) ?? "?";
        const i = steps.length;
        const step: Step = { kind: "dequeue", lane: at, label: event, log, payload };
        if (at !== undefined) {
          const q = inFlight.get(at) ?? [];
          // Deferred events can be dequeued out of order: match by event and payload.
          let j = q.findIndex((s) => s.event === event && s.key === key(payload));
          if (j < 0) j = q.findIndex((s) => s.event === event);
          if (j >= 0) {
            step.peer = q[j].step;
            steps[q[j].step].peer = i;
            q.splice(j, 1);
          }
        }
        steps.push(step);
        break;
      }
      case "MonitorProcessEvent":
        steps.push({
          kind: "monitor",
          lane: lane(str(d.monitor), true),
          to: laneIndex.get(str(d.sender) ?? ""),
          label: str(d.event) ?? "?",
          log,
          payload,
        });
        break;
      case "Announce":
        steps.push({ kind: "announce", lane: lane(str(d.id)), label: `announce ${str(d.event)}`, log, payload });
        break;
      case "RaiseEvent":
      case "MonitorRaiseEvent":
        steps.push({ kind: "raise", lane: lane(str(d.id) ?? str(d.monitor)), label: `raise ${str(d.event)}`, log });
        break;
      case "Print":
        steps.push({ kind: "print", label: log, log });
        break;
      case "Halt":
        steps.push({ kind: "halt", lane: lane(str(d.id)), label: "halt", log });
        break;
      case "AssertionFailure": {
        error = str(d.error) ?? log;
        // Put the bug on the machine it names (e.g. "Client(7) is waiting…"), else on
        // whoever acted last.
        const named = [...laneIndex.keys()]
          .filter((id) => error!.includes(id))
          .sort((a, b) => b.length - a.length)[0];
        const lastActor = [...steps].reverse().find((st) => st.lane !== undefined)?.lane;
        steps.push({ kind: "error", lane: named !== undefined ? laneIndex.get(named) : lastActor, label: error, log });
        break;
      }
      case "StrategyDescription":
        strategy = str(d.strategyDescription);
        break;
      case "GotoState":
      case "WaitMultipleEvents":
        break; // implied by the state and dequeue steps
      default:
        steps.push({ kind: "note", lane: lane(str(d.id) ?? str(d.monitor)), label: type, log, payload });
    }
  }
  // Machines first, monitors last.
  const order = lanes.map((_, i) => i).sort((a, b) => Number(lanes[a].monitor) - Number(lanes[b].monitor) || a - b);
  const remap = new Map(order.map((old, i) => [old, i]));
  const re = (i: number | undefined) => (i === undefined ? undefined : remap.get(i));
  return {
    lanes: order.map((i) => lanes[i]),
    steps: steps.map((s) => ({ ...s, lane: re(s.lane), to: re(s.to) })),
    error,
    strategy,
  };
}

/** Upstream prints payloads like `(<a:1, b:2, >)`; ours are JSON. Keep them short. */
export function formatPayload(p: unknown): string {
  if (p === undefined || p === null) return "";
  if (typeof p !== "object") return String(p);
  const s = JSON.stringify(p);
  return s.length > 60 ? s.slice(0, 57) + "…" : s;
}
