---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model scale-in with a load balancer, drain and terminate?
  - What happens to a request routed just before deregistration?
  - How do I check that termination never drops an in-flight request?
---

# Worker drain on scale-in

An autoscaler removes an instance from behind a load balancer in three steps: tell
the load balancer to deregister it and wait for the ack; tell the instance to drain
(finish in-flight requests, accept no new ones) and wait for "drained"; terminate it.
It refuses to remove the last instance. Clients send requests through the load
balancer. A request the load balancer routed just before deregistering travels
separately and can reach the instance after it started draining (the instance refuses
it) or after it was terminated (the connection is refused). Either way it comes back
to the load balancer, which routes it to an instance still in rotation.

**Contract**

- No request is routed to a deregistered or terminated instance, and a request is
  routed again only after its instance refused it or is gone.
- An instance drains only after the load balancer deregistered it, accepts nothing
  while draining, says nothing once terminated, and is terminated only after draining
  with nothing in flight (no accepted request is dropped).
- The last instance is never removed; a refusal is justified only for the last one.
- Progress: every request gets exactly one response, served by an instance, and every
  scale-in request ends with the instance terminated or the request refused.

**Atomic steps**: each machine handles one message at a time. Each routed request is a
separate `BBDelivery`, so it can arrive after later messages from other machines (the
drain command). Processing a request takes time (the instance's `Finish` to itself).

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `DrainSafety` (safety), `RequestsAndScaleInsFinish` (progress, hot state) |
| [`PSrc/Fleet.p`](PSrc/Fleet.p) | Load balancer, instance, autoscaler |
| [`PSrc/BrokenAutoscalers.p`](PSrc/BrokenAutoscalers.p) | Negative controls: terminate without waiting for drained; drain before the deregistration ack |
| [`PTst/Tests.p`](PTst/Tests.p) | Clients, the scale-in trigger, a scenario, the test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcScaleInUnderLoad` | Two clients, one request each, instance 0 removed | Requests in flight on the draining instance and on the other one | pass, sampled |
| `tcLateRequestDuringDrain` | One request at any moment relative to removing instance 1 | Routed before deregistration, arriving while draining or after termination: refused and rerouted | pass; check search completion |
| `tcLastInstanceRefused` | Remove 0, then 1; a client sends two requests | The second scale-in is refused; requests still served | pass; check search completion |
| `tcBugTerminateEarly` | `HastyAutoscaler` | **Expected to fail**: `instance 0 terminated with request 1 in flight` | fail |
| `tcBugDrainBeforeDeregistered` | `EarlyDrainAutoscaler` | **Expected to fail**: `instance 0 started draining while the load balancer still routes to it` | fail |

The `LateRequestRerouted` scenario confirms the checks reach a refused request that is
then served elsewhere.

## What building it taught

- **Track each delivery.** Mutation showed the load balancer's `Routed` and the
  instance's `Bounced` announcements could be dropped unnoticed. The safety spec now
  pairs every accept or refusal with the routing that sent it, and allows routing a
  request again only after a refusal or a termination.
- **A terminated instance must not speak.** Modeling "connection refused" as the
  terminated instance announcing a bounce made "forgot to move to Terminated"
  invisible. The refusal now reaches the load balancer with no announcement, and the
  spec rejects any announcement from a terminated instance.
- **Vary the target.** With only instance 0 ever removed, dropping `target = s.inst`
  was invisible (0 is the default); one test removes instance 1.

## Run

```sh
snappea check tcScaleInUnderLoad tcLateRequestDuringDrain tcLastInstanceRefused --timeout 30s --deny mutation
snappea check tcBugTerminateEarly             # expected bug (exit 0): ... terminated with request 1 in flight
snappea check tcBugDrainBeforeDeregistered    # expected bug (exit 0): ... started draining while the load balancer still routes to it
```

Search completion depends on the budget and explored interleavings. The checker
reports incomplete runs explicitly; `all N schedules` means the bounded search
finished. Neither coverage nor mutation detection implies exhaustive search.

## Bounds and omissions

Two instances, one or two clients, at most two requests, one or two scale-in requests
handled one at a time. Requests target distinct, existing instances; duplicate or
unknown instance removals are outside this model. Messages are delayed but not lost; requests always finish. Not
modeled: drain timeouts (a request that never finishes blocks the scale-in forever),
health checks, scale-out, client retries on timeout, connection draining inside the
load balancer itself.

## Testing the implementation

Emit `Routed`/`Deregistered` from the load balancer, `Accepted`/`Served`/
`Bounced`/`DrainStarted`/`Terminated` from the instance, and `ScaleInRefused` from
the autoscaler. To force the race: hold a request in a proxy between the load balancer
and the instance, deregister and drain the instance, then release the request (it must
be refused and rerouted); terminate after release and check the load balancer reroutes
on connection refused; hold a slow request in flight and check termination waits.

The project imports the reusable `BBDelivery` building block from the guide. Each
delivery gets an independent sender, allowing a request to arrive after drain.
When copying this model outside the repository, export `snappea guide example building-blocks`, then update the project include to its `Network.pproj`.
