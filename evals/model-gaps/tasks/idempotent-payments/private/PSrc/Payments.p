// Payment API with idempotency keys: the types and events. See DESIGN.md.
// These declarations are fixed: the test environment and the specs use them.

enum ChargeResult { Approved, Declined }

// ---- Client and API server ----

// One attempt of a payment. `client` is where the answer goes.
event Charge: (client: machine, key: int, amount: int);
// Final answer: the result of the key's charge.
event ChargeResponse: (key: int, result: ChargeResult);
// Not final: another request is still charging this key. Retry later (HTTP 409).
event ChargeConflict: (key: int);
// Final answer: the key was already used for a payment of another amount (HTTP 422).
event ChargeRejected: (key: int);

// ---- API server and key table ----

// Claimed:    the key was free; the caller now owns it and must charge.
// InProgress: another request owns the key and has not recorded a result.
// Finished:   the key's charge is recorded; `result` is its result.
// Mismatch:   the key was used with another amount.
enum ClaimStatus { Claimed, InProgress, Finished, Mismatch }

// `owner` is the id of the server that claims.
event Claim: (api: machine, owner: int, key: int, amount: int);
// `result` means something only with Finished.
event ClaimResponse: (key: int, status: ClaimStatus, result: ChargeResult);
// Record the result of the key's charge.
event Complete: (key: int, result: ChargeResult);
// Give up a key that is in progress and was never charged, so it can be claimed again.
event Release: (key: int);
// Asks for the keys that are in progress and owned by server `owner`.
event ListOwned: (api: machine, owner: int);
event OwnedKeys: seq[int];

// ---- API server and card processor (an external service) ----

// Charges the card. `api` is where the reply goes.
event Process: (api: machine, key: int, amount: int);
event ProcessResponse: (key: int, result: ChargeResult);
// To the API server: no reply to its Process for `key` arrived in time. The card may
// or may not have been charged.
event ProcessTimeout: (key: int);
// Asks whether the processor ran a charge for `key`.
event Lookup: (api: machine, key: int);
// `result` means something only when `found`.
event LookupResponse: (key: int, found: bool, result: ChargeResult);

// What a server process starts with: the key table, the card processor (or a network
// link in front of it; treat it as the processor), the id of the server it runs on,
// and whether it starts after a crash of the previous process on that server.
type ApiInit = (store: machine, processor: machine, id: int, restarted: bool);

// ---- Announced by the test environment, for the specs ----

// How many payments the run makes. Each payment has an id.
event PaymentsPlanned: int;
// A client sent an attempt (a Charge) of payment `payment`.
event Attempted: (payment: int, key: int, amount: int);
// A client received a ChargeResponse. `amount` is what its payment asked for.
event Answered: (payment: int, key: int, amount: int, result: ChargeResult);
// A client received a ChargeRejected.
event Rejected: (payment: int, key: int, amount: int);
// A client received a ChargeConflict. It will send another attempt.
event ToldToRetry: (payment: int, key: int);
// The card processor ran a charge.
event ProcessorCharged: (key: int, amount: int, result: ChargeResult);
