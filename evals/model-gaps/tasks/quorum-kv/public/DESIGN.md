# Quorum-replicated register

One key of a key-value store, kept on three replicas. Applications do not talk to the
replicas: each uses a coordinator (the client library), which runs one `Put` or `Get`
at a time against all three replicas and returns when two of them have answered. No
replica is special, there is no leader and no clock: versions order the writes.

A version is `(counter, writer)`. A version is newer than another if its counter is
higher, or the counters are equal and its writer id is higher. The register starts with
value 0 under version `(0, 0)` on every replica. Every put writes a distinct positive
value.

## How an operation runs

Both operations have a read phase and then a write phase.

1. **Read phase.** Send `ReplicaRead` to all three replicas. Wait until two different
   replicas have answered this operation. Take the newest answer.
2. **Write phase.** Send `ReplicaWrite` to all three replicas. Wait until two different
   replicas have acked this operation. Then return.
   - A `Put` writes its value under a new version: writer = the coordinator's id,
     counter = 1 + the highest counter among its two read answers and every counter
     this coordinator has ever used in a version.
   - A `Get` writes back exactly what it read (version and value), always: also when
     both answers agree and when it read the initial value. Then it returns that value.

A replica answers a read with what it holds. On a write it stores the offered
`(version, value)` only if the version is newer than the one it holds, and acks in
every case.

## Contract

- **The register is atomic.** A get returns the initial value or a value whose put
  started before the get returned. Call a value established once its put returned or
  some get returned it. A get that starts after a value was established returns that
  value or one written after it: never the initial value, and never an older value.
  So once any get returned a value, no later get goes back to an older one, even if
  the put of the newer value is still running or was abandoned.
- **An operation returns only what a write quorum has.** When a put or a get returns,
  two different replicas have handled a write of the returned version in that
  operation's write phase or before. A coordinator does not wait for the third
  replica: every operation returns while one replica never answers.
- **A replica keeps the newest version it was offered.** A late or repeated write
  never moves it back, and of two writes with the same counter the higher writer id
  wins on every replica. A replica acks every write it handles, stored or not: the
  coordinator is waiting.
- **A version names one write.** No two puts ever use the same version, and a put uses
  one version for all its messages, resends included. A put's version carries its
  coordinator's id and is newer than every version a put or get returned before the
  put started. This holds across a crash: the highest counter a coordinator has used
  is durable.
- **Only answers to the operation in flight count.** Requests carry the coordinator's
  operation number and answers carry it back. An answer or ack with another number
  (a late one, from an earlier operation or from before a crash) is dropped. Operation
  numbers are never reused, also not after a crash: the number is durable. Two answers
  from the same replica count once.
- **A timeout resends, nothing else.** On `OpTimeout` the coordinator sends the
  request of its current phase to all replicas again, with the same operation number
  and, in the write phase, the same version and value. It does not start the operation
  over. A timeout that arrives when no operation is in flight is dropped.
- **Every requested operation returns exactly once, unless a crash abandons it.** A
  crash discards the operation in flight: it never returns, and answers to it are
  dropped. The coordinator says whether there was one (`abandoned`), and is ready for
  the next request at once.

## Atomic steps

One handler each. Replica: answer a read; conditional write, then ack. Coordinator:
take a request and send the reads; handle one answer or ack (the last one of a phase
also starts the next phase or returns); handle a timeout; crash and restart. A
coordinator's operation number and the highest counter it used are durable; everything
about the operation in flight is volatile and lost in a crash. Everything between these
steps interleaves with the other coordinators and with the network.

## Messages and announcements

The events and types are declared in `PSrc/Events.p` and fixed.

A replica is created with its id: 1, 2 or 3. A coordinator is created with
`CoordinatorInit`: its owner (the application), its id (also its writer id) and the
three replicas in id order. An entry of `replicas` can be a network link in front of
the replica; treat it as the replica. Replies say which replica they come from in
their `replica` field; do not rely on which machine delivered them.

- `ReplicaRead` and `ReplicaWrite` carry `client = this` (the reply goes there) and
  `op`, the coordinator's operation number. `ReplicaReadResp` and `ReplicaWriteResp`
  carry the replica's id and the request's `op`.
- The owner sends `Put` with the value, or `Get`. It sends the next request only after
  the previous one returned or it was told of a restart that abandoned it. The
  coordinator answers `PutDone`, or `GetDone` with the value.
- `OpTimeout` and `Crash` come from the environment, at any time.
- After a crash the coordinator sends its owner `Restarted`, with
  `abandoned = true` if an operation was in flight.

The design announces these, for the specs:

| Event | Who, when | Payload |
| --- | --- | --- |
| `WriteStarted` | coordinator, at once when it takes a `Put`, before it sends anything | its id and the value |
| `WriteDone` | coordinator, when the put returns, just before `PutDone` | its id, the value and the version it was written with |
| `ReadStarted` | coordinator, at once when it takes a `Get`, before it sends anything | its id |
| `ReadDone` | coordinator, when the get returns, just before `GetDone` | its id, the value and the version it read and wrote back (`(0, 0)` for the initial value) |
| `Stored` | replica, every time it handles a `ReplicaWrite`, stored or not, before the ack | its id, the offered write, and what it holds afterwards |
| `CrashRecovered` | coordinator, every time it handles a `Crash`, just before `Restarted` | its id and `abandoned` |

Announce each once per occurrence, and do not `send` these events. The test
environment announces `PutRequested` and `GetRequested` when the application sends a
request.

## What can happen

One to three coordinators share the three replicas. Their operations run one after
another or at the same time, in any mix of puts and gets; two puts that overlap can
pick the same counter. A coordinator runs several operations in its life.

One replica can be down for the whole run and never answer.

The network between a coordinator and a replica can lose or delay any message:
a read, a write, a read answer or an ack. A delayed message arrives at any later
moment: after the operation it belongs to returned, during a later operation of the
same coordinator, after a crash, or after newer writes reached that replica. Whenever
a message is lost or delayed the coordinator gets an `OpTimeout`, itself at any later
moment: during the operation (in either phase), after it returned, or during the next
one. So a replica can get the same request twice and answer it twice, a coordinator
can be the only one a replica answers for a while, and a lost message is repaired only
by the resend. Messages are not corrupted, and the network does not duplicate them by
itself.

A coordinator can crash and restart, as one step, at any moment: when idle, in the
read phase or in the write phase, so a put can be left on no replica, one, or all.
Messages sent before the crash still arrive after it. Replicas do not crash and lose
nothing.

Not in scope: several keys, deletes, changing the replica set, replicas that lose
their state, more than one replica down, requests sent to a coordinator that is busy.
