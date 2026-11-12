---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do fencing tokens protect storage from a client whose lease expired?
  - What exactly does a fencing token guarantee, and what does it not?
  - How do I model a GC pause past a lease in P?
---

# Lease lock with fencing tokens

A lock service grants leases. Each grant carries a fencing token, one higher than the
last. A client writes to a storage service with its token, and storage accepts a write
only if its token is at least the highest it has seen. A client can stall after its
grant (a GC pause) for longer than the lease; meanwhile the lease expires, another
client acquires and writes, and then the stalled client wakes up and writes.

**Contract**

- At most one lease is live; tokens strictly increase; "busy" is answered only while
  a lease is live; release and expiry end only the live lease (a late release must not
  free the successor's lease).
- Storage accepts writes only with a token granted to that client, in token order.
  Once a later holder has written, no write from an earlier, superseded holder is
  accepted. A write is rejected only when a later token has already written.
- Every client the driver starts ends: it writes, is fenced, gives up after its retry
  budget, or (if planned) crashes. Every lease ends, by release or expiry.

**What fencing does not promise.** Storage only knows tokens it has seen. If the
paused holder wakes up *before* its successor's first write, its write is accepted,
and then ordered before the successor's. The model allows that order and checks it is
the only one. If you need "no write after the lease expired" at all, storage must ask
the lock service, or the lock service must push each new token to storage before
granting it.

**Atomic primitives**: the lock service's acquire, release and expire are one step
each; storage's compare-token-and-write is one step. The grant, the pause, each write
and the release are separate messages, and every lease's timer fires at a moment the
scheduler picks.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `FencedWrites` (safety) and `ClientsFinish` (progress, hot state) |
| [`PSrc/LockService.p`](PSrc/LockService.p) | Lock service and the one-shot `LeaseTimer` per lease |
| [`PSrc/Storage.p`](PSrc/Storage.p) | Storage with the fencing check |
| [`PSrc/Client.p`](PSrc/Client.p) | Acquire with bounded retry, write with token, release |
| [`PSrc/UnfencedStorage.p`](PSrc/UnfencedStorage.p) | Negative control: storage that ignores tokens |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, a scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoClientsContend` | Two clients ask once each; leases expire at any point | Busy answers, expiry between grant and write, late write before or after the successor's | pass, exhaustive |
| `tcHolderCrashes` | Client 1 dies holding the lease; client 2 tries twice | Only expiry frees the lock; a lost timer leaves the lease live forever (progress) | pass, exhaustive |
| `tcPausedHolderWritesLate` | Client 1 parks after its grant; its lease expires; client 2 acquires, writes twice, releases; then client 1 wakes and writes | The GC-pause story forced in order; the late write must be fenced; same-token writes accepted; client 1's late release must not free anything | pass, exhaustive |
| `tcBugStorageIgnoresTokens` | The pause story with `UnfencedStorage` | Negative control | fails: `write with token 1 accepted after token 2: superseded holder wrote` |

The `LateWriteFenced` scenario confirms the checks reach: expiry, a successor's write,
then a refused late write.

## Run

```sh
snappea check tcTwoClientsContend tcHolderCrashes tcPausedHolderWritesLate
snappea check tcBugStorageIgnoresTokens   # exits 1 with the late write above
```

`tcTwoClientsContend` explores every schedule in about 10 seconds, the others in a
few seconds.

## Bounds and omissions

Two clients, one grant per client, one or two writes per holder, at most two acquire
attempts. The pause is modeled two ways: any delay between messages (all tests), and
an explicit park/resume handshake (`tcPausedHolderWritesLate`). A crashed client still
reports `Crashed` to the driver as test bookkeeping. Not modeled: lock service
failover (tokens must survive it, or they can repeat), lease renewal, clock drift
between client and lock service, several storage replicas (each must fence), and
lost messages.

## Testing the implementation

Emit `eGranted`, `eBusy`, `eReleased`, `eExpired` from the lock service after each
state change, and `eWriteAccepted`/`eWriteRejected` from storage after its
conditional write; check them against `FencedWrites`. To force the GC-pause race,
block client 1 after acquire (a latch in the test), advance the lock service's clock
past the TTL, run client 2 to completion, then release the latch.
