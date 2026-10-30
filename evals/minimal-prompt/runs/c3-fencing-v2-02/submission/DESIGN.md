# Lock service with fencing tokens

## Contract
- acquire(c, now): if the lease record is unexpired (now < expires) and held by c -> its token (no extension);
  held by another -> None; else grant token = last_issued+1, expires = now+ttl.
- Tokens strictly increase over the store's life (`last` is never reset, not by release or expiry).
- release(c, token, now): True and lease ends iff c holds `token` and now < expires; else False, no effect.
- Resource.write(token, v): False (no effect) if token < 1, token > last issued, or token < the max token
  already accepted for that resource; else store v and True (equal token is accepted).
- Out-of-order `now` values are fine: every decision is a pure function of the stored record and the caller's now.

## Atomic primitives
Per-key load and compare_swap only. Keys: `lock` -> {last, holder, token, expires}; `res:<name>` -> {max, value}.
No cross-key atomicity: Resource.write reads `lock`.last (monotone, so a read is a valid lower bound on what the
caller's token could be: the caller got its token before calling write) and then CASes its own key.

## Scenarios
1. Two clients both read the same lock version; only one CAS wins, loser reloads and sees the lease held -> None.
2. Same client re-acquire while unexpired returns the same token and does not extend expiry.
3. Acquire exactly at now == expires is a fresh grant; at expires-1 it is not.
4. Release at now == expires fails; release with wrong token or wrong client fails.
5. Token after release/expiry is strictly greater (never reused).
6. Out-of-order now: acquire at an earlier time than the grant still sees the lease held.
7. Two Resource.write calls read the same version; the lower token must not overwrite the higher.
8. Write with never-issued token (0, negative, last+1) rejected; stale token rejected after higher accepted.
9. Two Resource objects / LockService objects share one store (state lives only in the store).
10. Different resources have independent max tokens.

## Model (Lock.p, Fence.p)
Lock.p: Store (version, record) with load/cas; 2 clients, each acquire, release, acquire with now in 0..3, ttl 2.
Spec computes expected results from the record at the version each operation observed.
Fence.p: Resource store (version, max, value); 2 writers with tokens 0..3, issued=2; spec tracks accepted max.
Not covered: real thread timing beyond forced interleavings; more than 2 clients; unbounded times.

## Found by the model
(see bottom, filled in after checking)
