# Payment API with idempotency keys

A client charges a card through a payment API and sends an idempotency key with each
payment. Answers get lost and users double click, so the same key arrives again: later,
or at the same moment, on the same API server or on another one. Two API servers run
behind a load balancer and share one key table. The card processor is an external
service; calling it is slow, its reply can be lost or late, and a server process can
crash in the middle of a call.

Each server process handles one request at a time: it claims the key in the key table,
calls the processor if it got the claim, records the result and answers. Requests that
arrive meanwhile wait their turn; none is dropped.

## Contract

- **One charge per key.** The processor is asked to charge a key at most once, ever,
  for the amount a request with that key asked for. A timeout, a crash, a retry or a
  decline never leads to a second `Process` for the key.
- **One result per key.** Every `ChargeResponse` for a key, to the first request, a
  retry, a concurrent duplicate or a request after a crash, carries the result of that
  key's one charge. No request is answered with a result before the processor ran the
  charge. A declined payment stays declined: the decline is recorded like an approval,
  and a retry with the same key returns `Declined` without calling the processor
  again, also after the card has funds again. A new key is a new payment.
- **A key belongs to one amount.** A request whose key was already used with another
  amount is answered `ChargeRejected`. It never gets the other payment's result and is
  never charged, whether the other payment is still in progress or done. A request is
  rejected only for that reason: never when its key was not used with another amount,
  and never when its own amount is the one the key was charged.
- **"Retry later" is honest.** `ChargeConflict` is sent only when another attempt with
  the same key holds the key in progress. A request whose key nobody else used is never
  told to retry.
- **Keys are independent.** A request is not delayed forever, refused or answered
  because of another key. A reply about one key never decides another key.
- **Every payment gets a final answer.** Every `Charge` a live process receives is
  answered: `ChargeResponse`, `ChargeRejected` or `ChargeConflict`. A client that is
  told to retry, or that times out, sends the request again, and in the end gets a
  `ChargeResponse` or a `ChargeRejected`. So a key never stays in progress forever:
  after a crash the restarted process settles the keys the crashed one held, whether or
  not any request reaches it.

## Atomic steps

The key table does one of these at a time, each a single conditional write or a read:

- `Claim(owner, key, amount)`: a free key becomes in progress, owned by server `owner`,
  for `amount`, and the caller is told `Claimed`. Otherwise nothing changes and the
  caller learns how the key stands: `Mismatch` (used with another amount), `Finished`
  with the recorded result, or `InProgress`.
- `Complete(key, result)`: an in-progress key becomes finished with `result`.
- `Release(key)`: an in-progress key becomes free again. A finished key stays.
- `ListOwned(owner)`: the keys in progress that server `owner` claimed.

Everything else interleaves: the claim, the processor call, recording the result and
the answer are separate steps, and the other server, the processor and the clients run
between them.

## Messages

The events and types are declared in `PSrc/Payments.p` and fixed.

A server process is created with `ApiInit`: the key table, the card processor (or a
network link in front of it; treat it as the processor), the id of its server (1 or 2)
and `restarted`. The key table is created without input.

- `Charge` carries the client (the answer goes there), the key and the amount.
- The server answers `ChargeResponse` (final, the key's result), `ChargeRejected`
  (final) or `ChargeConflict` (retry later).
- `Process` asks the processor to charge; it carries `api = this` for the reply. The
  processor is not idempotent: every `Process` it receives charges the card. It
  answers `ProcessResponse` with the key and `Approved` or `Declined`.
- `ProcessTimeout` tells a server that no reply to its `Process` for that key came in
  time. The card may have been charged.
- `Lookup` asks the processor whether it ran a charge for a key; `LookupResponse` says
  `found` and, if so, the result. A lookup never charges.

The test environment (the clients and the processor) announces what the specs observe:
`PaymentsPlanned`, `Attempted`, `Answered`, `Rejected`, `ToldToRetry` and
`ProcessorCharged`. The design announces nothing.

## What can happen

- **Duplicates.** The same key arrives twice at once, on two servers or twice on one,
  or again after it was answered. A client sends the same request again every time it
  is told to retry, without limit, to any server.
- **Other keys.** Different keys arrive at once on one server or on both. The card may
  have funds for only some of them, or for none; it can be topped up later.
- **Key reuse.** A key arrives with another amount than before, after the first
  payment is done or at the same moment on the other server.
- **Processor reply lost.** The server gets a `ProcessTimeout` and no reply. Requests
  to the processor are never lost, so the card was charged and a `Lookup` finds it.
- **Processor reply late.** The server gets a `ProcessTimeout`, and the
  `ProcessResponse` still arrives later: before or after the `LookupResponse`, after
  the request was answered, while the server is idle, or while it handles another key
  whose charge has another result. A `LookupResponse` can likewise arrive after a late
  `ProcessResponse` settled the request.
- **Server crash.** A server process can crash while it charges a key: after it
  claimed the key, and either before its `Process` left (the card is not charged) or
  after (the card is charged, the reply reaches nobody). The process is gone at once,
  with everything in memory and every request waiting in it; nothing it had not sent
  is sent. A new process starts on the same server with `restarted = true` and the
  same id. The key table and the processor keep what they have. The client times out
  and sends the request again, to the restarted server or the other one; the other
  server hears nothing of the crash. Other payments may arrive at the restarted
  process at once.
- **Order.** The processor and the key table handle requests in the order they were
  sent: a `Lookup` sent by a restarted process is answered after any `Process` the
  crashed process got out. Replies to a crashed process are lost.

Not in scope: a second crash of the same server, a crash combined with a lost or late
processor reply, lost requests, more than one faulty reply per run, a crash of the key
table or the processor, key expiry, refunds.
