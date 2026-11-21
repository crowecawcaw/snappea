// Scale-in behind a load balancer: the types and events. See DESIGN.md.
// These declarations are fixed: the test environment and the specs use them.

type WorkRequest = (id: int, client: machine);

// Why the autoscaler refused a scale-in request.
//   NotInService: the instance is not one the autoscaler could remove: it was removed
//                 already, or the autoscaler never had it.
//   LastInstance: it is the only instance left in service.
enum RefuseReason { NotInService, LastInstance }

// ---- Messages ----

// Something that takes time: a message in transit, or work in progress. A new Delivery
// sends `message` with `payload` to `destination` in a step of its own, so it can
// arrive after messages that were sent later.
type Parcel = (destination: machine, message: event, payload: any);

machine Delivery {
  start state Deliver {
    entry (p: Parcel) {
      send p.destination, p.message, p.payload;
    }
  }
}

// Client -> load balancer, and the instance's answer to the client.
event Request: WorkRequest;
event Response: (id: int);
// Load balancer -> instance, through a Delivery: serve this request.
event Work: WorkRequest;
// Instance -> load balancer: I did not take this request; route it elsewhere.
event Bounce: WorkRequest;
// Environment -> load balancer, once, before any request: the instances in rotation.
event Join: map[int, machine];
// Autoscaler -> load balancer: stop routing to `inst`; answer `from`.
event Deregister: (inst: int, from: machine);
event DeregisterAck: (inst: int);
// Autoscaler -> instance: finish what you have, take nothing new, tell me (the
// payload) when nothing is in flight.
event Drain: machine;
event Drained: (inst: int);
// Autoscaler -> instance: stop.
event Terminate;
// Instance to itself, through a Delivery: the work for request `id` is done.
event Finish: int;
// Environment -> autoscaler: remove instance `inst`.
event ScaleIn: (inst: int);
// Environment -> autoscaler: the timer of the step in progress fired.
event StepTimeout;

// ---- Announced by the design, for the specs ----

// Load balancer: sent request `id` to instance `inst`.
event Routed: (id: int, inst: int);
// Load balancer: took `inst` out of rotation; it routes nothing there from now on.
event Deregistered: (inst: int);
// Instance: took request `id` / finished it and answered the client / refused it
// because it is draining.
event Accepted: (id: int, inst: int);
event Served: (id: int, inst: int);
event Bounced: (id: int, inst: int);
// Instance: started draining / was terminated.
event DrainStarted: (inst: int);
event Terminated: (inst: int);
// Autoscaler: refused to remove `inst`.
event ScaleInRefused: (inst: int, reason: RefuseReason);

// ---- Announced by the test environment ----

// Instances 0..instances-1 are in rotation; clients will send `requests` requests and
// the environment will ask for `scaleIns` scale-ins.
event FleetPlanned: (instances: int, requests: int, scaleIns: int);
// A client sent request `id` / got its response.
event RequestSent: (id: int);
event GotResponse: (id: int);
// The environment asked the autoscaler to remove `inst`.
event ScaleInRequested: (inst: int);
