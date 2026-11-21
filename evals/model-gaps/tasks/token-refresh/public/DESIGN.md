# Token refresh across browser tabs

The tabs of a web app share one login: a refresh token in `localStorage`. When a tab's
access token expires it needs a fresh one. The auth server rotates refresh tokens:
each refresh returns a new token and the old one is no longer valid. Presenting a
refresh token that was already rotated out is treated as theft: the server revokes
the whole token family, and the user is logged out everywhere.

Tokens are numbered. Refresh token n comes with access token n; a refresh with n
issues n + 1.

The design is the two machines in `PSrc/`: `Tab` (what a tab does when its token
expires) and `AuthServer` (rotation and reuse detection for one family). `PSrc/Api.p`
holds the events, which are fixed. Storage, the cross-tab lock and the network are the
browser platform: `PTst/Browser.p` models them, and they are not yours to change in
the product. The hidden tests bring their own copy, which does everything "What can
happen" lists.

## Contract

- The server accepts only the current refresh token, never a rotated-out one. Each
  refresh issues a newer token.
- Presenting a rotated-out refresh token revokes the family, however long ago the
  token was rotated out. After that the server accepts no token of the family.
- A legitimate user with only these tabs is never logged out by reuse detection: the
  family is revoked only after an attacker presented a stolen token.
- A tab uses only a token the server issued, and newer than the one that expired.
- Single flight: a token is refreshed only if a tab still waiting holds it as its
  expired token. A tab never refreshes when a newer token is already stored.
- Every tab whose token expired gets a fresh one. The only excuses are a response
  that missed its request's timer and a revoked family: then tabs may give the
  session up, and the user logs in again. A tab never gives up without one of those.

A tab announces `TokenObtained` once per expiry when it has the fresh token, or
`SessionLost` once when it gives the session up; after that it needs no more tokens.
The server announces `Rotated` and `FamilyRevoked`.

With a server this strict, a response that missed its timer cannot be recovered: the
server may have rotated the token, the tab does not know the new one, and presenting
the old one again would be a reuse. Giving the session up is the accepted outcome.
Storage has a `lost` flag so tabs can tell each other.

## Atomic steps

One handler of the auth server (a transaction on the family's row). One storage read,
or one write of the whole session. A lock acquire; a lock release. Everything else
interleaves: a tab's read, its lock request, its refresh and its write are separate
steps, and other tabs, the network and an attacker run in between.

## What can happen

- Any number of tabs, each with a token that expires any number of times, at the same
  moment as other tabs' or later. A tab starts a new round only after the previous
  one ended.
- A request always reaches the server. Its response can miss the request's timer: the
  tab gets `RefreshTimeout`, and the response then never arrives or arrives later.
- A response can arrive in time and the request's timer still fire, at any later
  moment, also while the tab's next refresh is in flight. `RefreshTimeout` names the
  refresh token its request presented.
- An attacker with a copy of the login refresh token presents it to the server
  directly, once, at any moment: before the tabs' first refresh, between refreshes,
  or after them.
- Storage and the lock do not fail, tabs do not close, and messages other than
  responses are not lost.

Not in scope: tabs closing while they hold the lock, lost requests, several logins,
server-side grace periods or idempotent refresh, logging in again.
