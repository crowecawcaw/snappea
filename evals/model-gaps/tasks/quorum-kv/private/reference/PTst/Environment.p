// The test environment: the application that drives the coordinators, the network
// between a coordinator and a replica, a replica that is down, and crash injection.
// Coordinators and replicas are the design under test (PSrc).

// A replica that is down for the whole run: it never answers.
machine Unreachable {
  start state Down {
    ignore ReplicaRead, ReplicaWrite;
  }
}

// Sends one message from its own machine, so the scheduler can deliver it before or
// after anything sent later: a message that arrives late.
machine Courier {
  start state Deliver {
    entry (packet: (destination: machine, message: event, payload: any)) {
      send packet.destination, packet.message, packet.payload;
    }
  }
}

// A timeout for a coordinator, delivered at any later moment: during the operation,
// or after it has returned.
machine TimeoutTimer {
  start state Fire {
    entry (coordinator: machine) { send coordinator, OpTimeout; }
  }
}

// A crash for a coordinator, delivered at any later moment.
machine Crasher {
  start state Fire {
    entry (coordinator: machine) { send coordinator, Crash; }
  }
}

// Which messages between one coordinator and one replica a fault hits.
enum Leg { ReadRequest, WriteRequest, ReadAnswer, WriteAck }
// Lose: the message is dropped. Delay: it arrives at any later moment.
enum Action { Lose, Delay }
// The network between coordinator `client` and replica `replica` applies `action` to
// the first `times` messages of kind `leg`. Each time, the coordinator also gets an
// OpTimeout (at any later moment).
type Fault = (client: int, replica: int, leg: Leg, action: Action, times: int);

// The network between one coordinator and one replica. The coordinator is given the
// link in place of the replica. Messages a fault does not hit pass in order.
machine Link {
  var replica: machine;
  var client: machine;
  var fault: Fault;

  start state Forwarding {
    entry (input: (replica: machine, fault: Fault)) {
      replica = input.replica;
      fault = input.fault;
    }
    on ReplicaRead do (r: (client: machine, op: int)) {
      client = r.client;
      Pass(ReadRequest, replica, ReplicaRead, (client = this, op = r.op));
    }
    on ReplicaWrite do (w: (client: machine, op: int, write: Stamped)) {
      client = w.client;
      Pass(WriteRequest, replica, ReplicaWrite, (client = this, op = w.op, write = w.write));
    }
    on ReplicaReadResp do (r: (replica: int, op: int, held: Stamped)) {
      Pass(ReadAnswer, client, ReplicaReadResp, r);
    }
    on ReplicaWriteResp do (a: (replica: int, op: int)) {
      Pass(WriteAck, client, ReplicaWriteResp, a);
    }
  }

  fun Pass(leg: Leg, target: machine, message: event, payload: any) {
    if (fault.times > 0 && fault.leg == leg) {
      fault.times = fault.times - 1;
      new TimeoutTimer(client);
      if (fault.action == Delay) {
        new Courier((destination = target, message = message, payload = payload));
      }
      return;
    }
    send target, message, payload;
  }
}

// One request of the application: a put of `value`, or a get (value 0), to coordinator
// `client`. With `crash`, that coordinator is also crashed at some later moment:
// before, during or after this operation, or during a later one.
type Step = (client: int, value: int, crash: bool);

// A run: `clients` coordinators (ids 1..clients) on three replicas, replica `down`
// (0: none) never answering, the network faults, and the application's requests.
// The steps of one lane run one after another: the next is sent when the previous
// returned or was abandoned by a crash. Lanes run concurrently. A coordinator belongs
// to one lane.
type Plan = (clients: int, down: int, faults: seq[Fault], lanes: seq[seq[Step]]);

// The application. It owns the coordinators.
machine Driver {
  var coordinators: map[int, machine];
  var lanes: seq[seq[Step]];
  // lane -> index of its step in flight
  var position: seq[int];
  var laneOf: map[int, int];

  start state Running {
    entry (plan: Plan) {
      var replicas: seq[machine];
      var path: seq[machine];
      var f: Fault;
      var st: Step;
      var i: int;
      var c: int;
      var lane: int;
      i = 1;
      while (i <= 3) {
        if (i == plan.down) {
          replicas += (i - 1, new Unreachable());
        } else {
          replicas += (i - 1, new Replica(i));
        }
        i = i + 1;
      }
      c = 1;
      while (c <= plan.clients) {
        path = replicas;
        foreach (f in plan.faults) {
          if (f.client == c) {
            path[f.replica - 1] = new Link((replica = replicas[f.replica - 1], fault = f));
          }
        }
        coordinators[c] = new Coordinator((owner = this, id = c, replicas = path));
        c = c + 1;
      }
      lanes = plan.lanes;
      while (lane < sizeof(lanes)) {
        foreach (st in lanes[lane]) {
          assert !(st.client in laneOf) || laneOf[st.client] == lane, "a coordinator belongs to one lane";
          laneOf[st.client] = lane;
        }
        position += (lane, 0);
        lane = lane + 1;
      }
      lane = 0;
      while (lane < sizeof(lanes)) {
        Request(lane);
        lane = lane + 1;
      }
    }
    on PutDone do (r: (client: int)) { Advance(r.client); }
    on GetDone do (r: (client: int, value: int)) { Advance(r.client); }
    on Restarted do (r: (client: int, abandoned: bool)) {
      if (r.abandoned) { Advance(r.client); }
    }
  }

  fun Advance(client: int) {
    var lane: int;
    lane = laneOf[client];
    position[lane] = position[lane] + 1;
    Request(lane);
  }

  fun Request(lane: int) {
    var st: Step;
    if (position[lane] >= sizeof(lanes[lane])) { return; }
    st = lanes[lane][position[lane]];
    if (st.value > 0) {
      announce PutRequested, (client = st.client, value = st.value);
      send coordinators[st.client], Put, st.value;
    } else {
      announce GetRequested, (client = st.client,);
      send coordinators[st.client], Get;
    }
    if (st.crash) { new Crasher(coordinators[st.client]); }
  }
}

// ---- Helpers to write plans ----

fun PutStep(client: int, value: int): Step { return (client = client, value = value, crash = false); }
fun GetStep(client: int): Step { return (client = client, value = 0, crash = false); }
fun Crashing(st: Step): Step { return (client = st.client, value = st.value, crash = true); }

fun NoFaults(): seq[Fault] { return default(seq[Fault]); }
fun NoLanes(): seq[seq[Step]] { return default(seq[seq[Step]]); }
fun NoSteps(): seq[Step] { return default(seq[Step]); }

fun AndFault(faults: seq[Fault], f: Fault): seq[Fault] {
  faults += (sizeof(faults), f);
  return faults;
}
fun AndStep(steps: seq[Step], st: Step): seq[Step] {
  steps += (sizeof(steps), st);
  return steps;
}
fun AndLane(lanes: seq[seq[Step]], lane: seq[Step]): seq[seq[Step]] {
  lanes += (sizeof(lanes), lane);
  return lanes;
}
