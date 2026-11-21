# Worker drain on scale-in

Instances serve requests behind a load balancer. An autoscaler removes one without
dropping a request: the load balancer stops routing to it (`Deregister`,
`DeregisterAck`), the instance finishes what it has and takes nothing new (`Drain`,
`Drained`), then it is stopped (`Terminate`). `LoadBalancer`, `Instance` and
`Autoscaler` are the design. Clients, scale-ins and the step timer are the environment.

## Contract

- Every `Request` gets exactly one `Response`, from an instance that accepted it and
  finished it. An instance announces `Accepted` when it takes a request and `Served`
  when it finishes one, then answers the client. No accepted request is dropped.
- The load balancer announces `Routed` each time it sends an instance a request (`Work`,
  unchanged). It routes only to an instance in rotation: one `Join` gave it and it has
  not deregistered. It routes a request again only when the instance gave it back
  (`Bounce`). An instance takes or refuses only what was routed to it.
- An instance's life goes one way: in rotation, deregistered, draining, terminated. The
  load balancer announces `Deregistered` when it takes an instance out of rotation, the
  instance `DrainStarted` when it starts draining and `Terminated` when it is stopped:
  each once per instance, however often the command arrives. An instance starts draining
  only once deregistered, and is terminated only while draining with nothing in flight.
- A draining instance accepts nothing. It refuses a request that still reaches it
  (`Bounced`, and the request goes back to the load balancer) and finishes those it had
  accepted. An instance refuses only while draining. `Drained` is true when sent: the
  instance is draining with nothing in flight. It goes to whoever asked last.
- A terminated instance announces nothing. A request that still reaches it finds the
  connection refused: it goes back to the load balancer as `Bounce`, with no `Bounced`.
- Every `ScaleIn` ends in exactly one of two ways: that instance is `Terminated`, or the
  autoscaler announces `ScaleInRefused`. Neither happens without a request. Each ends
  even if the step timer never fires. Removals run one at a time; a request arriving
  during one waits, then is decided, in arrival order, on the fleet as it then is.
- An instance is in service from the start until the autoscaler starts removing it. A
  request for an instance not in service (removed, or never in the fleet) is refused
  with `NotInService`, however many are left; one for the only instance in service, with
  `LastInstance`. Any other is carried out. The last instance is never deregistered.
- A repeated command has no second effect, and is answered again when the answer is
  true. An answer that is repeated or late (its step is over, or another instance is now
  being removed) must not advance anything. Nothing that can arrive is an error.

## Atomic steps

The load balancer gets `Join` once, before any request: the rotation by id. An entry can
be a network link to the instance; it takes only `Work`. The autoscaler starts with
every instance by id, all in service. Two things take time and go through a new
`Delivery`: a `Work` in transit, and the work itself (`Finish`, instance to itself).

## What can happen

Fleets have two or three instances and up to two clients, or none. A client sends up to
three requests, one at a time. Scale-in requests arrive at any moment, also during a
removal. They can name the same instance twice, an id the fleet never had, and every
instance in turn. The step timer fires at any moment, any number of times: when the
awaited answer is only slow, between removals, when nothing was asked. When it fires
during a removal, the autoscaler repeats the command it awaits an answer to. So answers
come twice, and the second can arrive in the same step, after the removal, or during the
next one. A request can reach an instance that is draining or terminated; a drain can
start with requests in flight. Messages are not lost; requests always finish.

Not in scope: crashes, restarts, scale-out, health checks, drain deadlines, retries.
