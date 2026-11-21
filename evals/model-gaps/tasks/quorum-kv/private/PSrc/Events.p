// Quorum-replicated register: the types and events. See DESIGN.md.
// These declarations are fixed: the test environment and the specs use them.

// A write's version: its counter, then the writer's id to break ties. (0, 0) is the
// version of the initial value.
type Version = (counter: int, writer: int);
// A value and the version of the write that wrote it. The register starts as
// ((0, 0), 0).
type Stamped = (version: Version, value: int);

// ---- Coordinator and replica ----

// client: where the answer goes. op: the coordinator's number for the operation this
// request belongs to; the answer carries it back.
event ReplicaRead: (client: machine, op: int);
// replica: the answering replica's id. held: what it holds.
event ReplicaReadResp: (replica: int, op: int, held: Stamped);
event ReplicaWrite: (client: machine, op: int, write: Stamped);
event ReplicaWriteResp: (replica: int, op: int);

// ---- Application (the coordinator's owner) and coordinator ----

// Write this value. Values are positive and no two puts use the same value.
event Put: int;
// Read the register.
event Get;
event PutDone: (client: int);
event GetDone: (client: int, value: int);
// The coordinator restarted. abandoned: an operation was in flight and will never
// return.
event Restarted: (client: int, abandoned: bool);

// ---- Environment to coordinator ----

// The operation in flight is taking long: resend its current request.
event OpTimeout;
// Crash and restart, as one step.
event Crash;

// What a coordinator is created with: whom to answer, its id (also its writer id),
// and the three replicas in id order (1, 2, 3).
type CoordinatorInit = (owner: machine, id: int, replicas: seq[machine]);

// ---- Announced by the design, for the specs ----

// Coordinator: it took a Put of `value` and starts working on it.
event WriteStarted: (client: int, value: int);
// Coordinator: the put returned (just before PutDone). version: the write's version.
event WriteDone: (client: int, value: int, version: Version);
// Coordinator: it took a Get and starts working on it.
event ReadStarted: (client: int);
// Coordinator: the get returned `value` (just before GetDone). version: the version
// that value was read with.
event ReadDone: (client: int, value: int, version: Version);
// Replica, every time it handles a ReplicaWrite: the write it was offered and what it
// holds afterwards.
event Stored: (replica: int, write: Stamped, holds: Stamped);
// Coordinator: it crashed and restarted (just before Restarted, same payload).
event CrashRecovered: (client: int, abandoned: bool);

// ---- Announced by the test environment ----

// The application asked coordinator `client` for a put of `value` / for a get.
event PutRequested: (client: int, value: int);
event GetRequested: (client: int);
