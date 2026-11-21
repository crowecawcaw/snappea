# Worker drain on scale-in

A fleet of worker instances serves requests behind a load balancer. An autoscaler
removes an instance without dropping a request, in three steps: it tells the load
balancer to deregister the instance and waits for the ack; it tells the instance to
drain (finish what is in flight, take nothing new) and waits for "drained"; it
terminates the instance. It removes one instance at a time and never the last one.

Three machines make up the design: `LoadBalancer`, `Instance` (one per worker) and
`Autoscaler`. Clients, whoever asks for scale-ins, and the autoscaler's step timer are
the environment.

## Contract

- **Every request is answered once.** Each request a client sends gets exactly one
  response, from an instance that accepted it and finished it. No accepted request is
  dropped: an instance is terminated only with nothing in flight.
- **Routing follows the rotation.** The load balancer routes only to instances in
  rotation, never to one it deregistered or one that was terminated. It routes a request
  again only when the instance it went to gave it back: the instance refused it, or is
  gone.
- **An instance's life goes one way.** In rotation, deregistered, draining, terminated,
  each step once. An instance starts draining only after the load balancer deregistered
  it. While draining it accepts no request: it refuses every request that still
  arrives and finishes those it had accepted. It refuses requests only while draining.
  It is terminated only while draining, with nothing in flight. Once terminated it
  announces nothing.
- **Scale-ins run one at a time and each one ends.** Every scale-in request ends with
  the instance terminated or with a refusal. A request that arrives while a removal is
  in progress waits for it and is then decided on the fleet as it is at that moment.
- **Refusals are justified, and say why.** A request for an instance that is not in
  service (already removed, or never part of the fleet) is refused with `NotInService`.
  A request for an instance that is in service is refused with `LastInstance` when it
  is the only one in service, and is carried out otherwise. Which reason applies is
  decided in that order: an instance that is not in service is `NotInService`, however
  many are left. The last instance is never deregistered.
- **Repeats and late answers change nothing.** The step timer can fire at any moment
  (see below), so commands are repeated and answers arrive twice or late. A repeated
  command must not repeat its effect, and an answer about an instance other than the
  one being removed, or one that arrives when no step waits for it, is ignored.

## The machines

The events are declared in `PSrc/Fleet.p` and fixed.

**Load balancer.** Created with nothing. It gets `Join` once, with the instances in
rotation by id, before any request. An entry can be a network link in front of the
instance; treat it as the instance.

- `Request` (from a client) and `Bounce` (a request given back): route it. Pick any
  instance in rotation, announce `Routed`, send the instance `Work` with the request
  unchanged, through a new `Delivery`.
- `Deregister (inst, from)`: if `inst` is in rotation, take it out and announce
  `Deregistered`. In every case send `DeregisterAck (inst)` to `from`; a repeated
  `Deregister` is only acknowledged again.

**Instance.** Created with `(id, lb)`.

- Serving. `Work`: accept it, announce `Accepted`, and send itself `Finish` with the
  request id through a new `Delivery`: the work takes time. `Finish`: announce
  `Served` and send the client `Response`.
- `Drain` (payload: who to tell) while serving: announce `DrainStarted` and start
  draining.
- Draining. `Work`: refuse it, announce `Bounced`, and send the request back to the
  load balancer as `Bounce`. `Finish`: as when serving.
- It sends `Drained (inst)` to whoever sent the last `Drain` whenever it is draining and
  holds no request in flight, at these moments: when it starts draining, when a request
  finishes while it drains, and when it gets `Drain` again while draining. A repeated
  `Drain` does not announce `DrainStarted` again, and gets no `Drained` while a request
  is in flight.
- `Terminate` while draining: announce `Terminated`. The autoscaler sends it only after
  `Drained`.
- Terminated. The process is gone. A `Work` that still arrives gets "connection
  refused": model it as the instance sending `Bounce` with the request to the load
  balancer **without announcing anything**. Any other message is dropped.

**Autoscaler.** Created with `(lb, instances)`: the load balancer and every instance by
id. All of them are in service at the start. An instance stops being in service when
the autoscaler starts removing it.

- `ScaleIn (inst)` with no removal in progress: refuse it by announcing
  `ScaleInRefused` with the reason (see the contract), or start the removal: send the
  load balancer `Deregister (inst, from = this)`.
- `DeregisterAck` for the instance being removed, while waiting for it: send that
  instance `Drain` with `this`.
- `Drained` from the instance being removed, while waiting for it: send it `Terminate`.
  The removal is over; the next waiting `ScaleIn` is decided.
- `StepTimeout`: the timer of the step in progress fired. Waiting for the ack, send
  `Deregister` again; waiting for `Drained`, send `Drain` again. With no removal in
  progress it means nothing.
- `ScaleIn` during a removal waits. Requests are decided in the order they arrived.

## Announcements

The design announces these, for the specs:

| Event | Who, when | Payload |
| --- | --- | --- |
| `Routed` | load balancer, every time it sends a `Work` | request id, instance id |
| `Deregistered` | load balancer, when it takes an instance out of rotation, once per instance | instance id |
| `Accepted` | instance, when it takes a request | request id, instance id |
| `Served` | instance, when a request finishes, before it sends `Response` | request id, instance id |
| `Bounced` | instance, when it refuses a request while draining | request id, instance id |
| `DrainStarted` | instance, on the first `Drain` | instance id |
| `Terminated` | instance, on `Terminate` | instance id |
| `ScaleInRefused` | autoscaler, when it refuses a scale-in request | instance id, reason |

The test environment announces `FleetPlanned` (instances `0..instances-1` are in
rotation and in service; how many requests the clients will send and how many scale-ins
will be asked for), `RequestSent` and `GotResponse` (a client sent a request, got its
response) and `ScaleInRequested` (it sent the autoscaler a `ScaleIn`).

## Atomic steps

Each machine handles one message at a time, in the order the messages reached it. Two
things take time and are modeled with `Delivery` (declared in `PSrc/Fleet.p`; a new
`Delivery` passes on one message in a step of its own):

- A `Work` in transit. The load balancer sends each `Work` through a new `Delivery`, so
  a request routed just before the instance was deregistered can reach it after the
  autoscaler's `Drain`, or after its `Terminate`.
- The work itself. An instance sends itself each `Finish` through a new `Delivery`, so
  requests finish in any order and a `Drain`, a repeated `Drain` or another `Work` can
  reach the instance while a request is in flight.

Every other message is sent directly.

## What can happen

- Two or three instances. One or two clients, or none; each sends one to three
  requests, the next after the previous response. Request ids are unique.
- Scale-in requests arrive at any moment: before, between and after requests, and
  while another removal is in progress. They can name the same instance twice, an
  instance that is already gone, an id the fleet never had, and every instance of the
  fleet one after the other.
- The step timer fires at any moment, any number of times: while the answer the
  autoscaler waits for is only slow, during either step, between removals, and when
  nothing was ever asked. So the load balancer can get `Deregister` twice and the
  instance `Drain` twice, with a request in flight or not, and the autoscaler gets two
  acks or two `Drained` for one instance. The second can arrive during the same step,
  after the removal is over, or during either step of the next removal, when the
  instance now being removed may still have requests in flight.
- A request can be in flight on an instance when its drain starts, and a routed request
  can reach an instance while it drains or after it is terminated.
- Messages are not lost and not duplicated by the network. Requests always finish.

Not in scope: instance crashes, scale-out, health checks, a drain deadline that gives
up on slow requests, client retries, a restart of the autoscaler or the load balancer.
