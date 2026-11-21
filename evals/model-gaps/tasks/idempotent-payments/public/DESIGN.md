# Payment API with idempotency keys

A client charges a card through a payment API and sends an idempotency key with each
payment. Answers get lost and users double click, so the same key arrives again: later,
or at the same moment, on the same API server or on another one. Two API servers
(`PaymentApi`) behind a load balancer share one key table (`KeyStore`). The card
processor is external and not idempotent: every `Process` it receives charges the
card. Its reply can be lost or late, and a server process can crash during a call.

## Contract

- The processor gets at most one `Process` per key, ever, for an amount a request with
  that key asked for. A timeout, a crash, a retry or a decline never leads to a second.
- Every `ChargeResponse` for a key carries the result of that key's one charge: to the
  first request, a retry, a concurrent duplicate or a request after a crash. None is
  sent before the charge ran. A declined key stays declined, even with new funds.
- A request whose key was used with another amount is answered `ChargeRejected`,
  whether the other payment is in progress or done. It never gets that payment's
  result and is never charged. A request is rejected for no other reason, and never
  when its own amount is the one the key was charged.
- `ChargeConflict` is sent only when another attempt with the same key holds the key
  in progress. A request whose key nobody else used is never told to retry.
- Every `Charge` a live process receives is answered, also one that arrives while it
  is busy or has just restarted. A client that is told to retry, or times out, sends
  again and in the end gets a `ChargeResponse` or a `ChargeRejected`: no key stays in
  progress forever, also when no request for it ever reaches a restarted process.
- Keys are independent: no request waits forever, is refused or is answered because of
  another key. A reply that arrives after its payment was settled another way must not
  change any payment.

## Atomic steps

The key table does one `Claim`, `Complete`, `Release` or `ListOwned` at a time; each is
one conditional write or a read. Nothing else is atomic: the other server, the
processor and the clients run between a server's writes to the key table, its call to
the processor and its answer. The key table is durable; a server process is not.

## What can happen

The clients and the processor announce what the specs observe; the design announces
nothing. The same key arrives twice at once, on two servers or twice on one, or again
after it was answered, while other keys wait behind it. A retry can go to any server,
without limit. Different keys arrive at once, on one server or both. The card may have
funds for only some of them, or none, and can be topped up later. A key arrives with
another amount, after the first payment is done or at once on the other server, and
then again with the first amount. The processor's reply can be lost: the server gets a
`ProcessTimeout` and nothing more. Or late: the `ProcessResponse` arrives after the
`ProcessTimeout`, before or after the answer to anything the server asked meanwhile,
while the server is idle or while it handles another key whose charge has another
result. Requests to the processor and the key table are never lost and are handled in
the order sent, also across a crash; the processor remembers every charge it ran. A
server process can crash after it claimed a key: before its `Process` left (the card is
not charged) or after (it is charged, the reply reaches nobody). It is gone at once,
with every request waiting in it. A new process starts on that server with the same id
and `restarted = true`. The client times out and sends again: to the restarted server,
to the other one, or only ever to the other one, which hears nothing of the crash.
Other keys may arrive at the restarted process at once.

Not in scope: a second crash, a crash together with a lost or late reply, a second
faulty reply, a crash of the key table or the processor, key expiry, refunds.
