# Quorum-replicated register

One key of a key-value store, kept on three replicas. Applications do not talk to the
replicas: each uses a coordinator (the client library), which runs one `Put` or `Get`
at a time against the replicas. No replica is special, there is no leader and no
clock: versions order the writes. A version is `(counter, writer)`; it is newer than
another if its counter is higher, or the counters are equal and its writer id is
higher. Every replica starts with value 0 under version `(0, 0)`. Coordinators can
crash, one replica can be down, and messages are lost or delayed.

## Contract

- The register is atomic. A get returns the initial value or a value whose put
  started before the get returned. A value is established once its put returned or
  some get returned it. A get that starts after a value was established returns that
  value or one written after it: never the initial value, never an older one, even if
  the newer value's put is still running or was abandoned.
- Before a put or a get returns, two different replicas have each handled a
  `ReplicaWrite` of the returned version. A replica's initial state is not a write.
- After every write it handles, a replica holds the newest version it was ever
  offered. A late or repeated write never moves it back.
- A version names one write. No version ever carries two values and no value is ever
  written under two versions; puts that a crash abandoned count too. A put's version
  has its coordinator's id as writer and is newer than every version a put or get
  returned before the put started. A get returns a value with the version it was
  written under (`(0, 0)` for the initial value).
- Every requested operation starts once and returns once, also when one replica never
  answers and when messages were lost. Only a crash abandons it: then it never
  returns. After a crash the coordinator sends its owner `Restarted`, `abandoned`
  true exactly when an operation was in flight, and takes the next request at once.
- A late answer, ack or timeout never completes or changes another operation, also
  not after a crash, and changes nothing when no operation is in flight.
- The design announces, once each and never with `send`: `WriteStarted` or
  `ReadStarted` in the step that takes the `Put` or `Get`; `WriteDone` or `ReadDone`
  in the step that returns (`PutDone`, `GetDone`), with the value and its version;
  `Stored` whenever a replica handles a `ReplicaWrite`, stored or not, with the write
  and what it holds after; `CrashRecovered` at every `Crash`, payload as `Restarted`.

## Atomic steps

One handler each. Replica: answer a read; handle a write. Coordinator: take a request;
handle one answer or ack; handle a timeout; crash and restart. A coordinator can keep
durable state across a crash; the operation in flight is volatile and is lost.

## What can happen

The events and types are in `PSrc/Events.p` and fixed; its comments say what each
field carries. One to three coordinators share the replicas; their operations run in
turn or at the same time, in any mix. Every put writes a distinct positive value. An
owner sends its next request only after the previous one returned or a restart
abandoned it. One replica can be down for the whole run. An entry of `replicas` can be
a network link in front of the replica; treat it as the replica. The network can lose
or delay any message, in either direction, and never resends. A delayed message
arrives at any later moment: during a later operation, after a crash, or after newer
writes reached that replica. For each lost or delayed message the coordinator gets one
`OpTimeout`, itself at any later moment. A coordinator can crash and restart, as one
step, at any moment, idle or not, so a put can be left on no replica, one, or all.

Not in scope: several keys, deletes, changing the replica set, replicas that lose
their state, more than one replica down, requests to a busy coordinator.
