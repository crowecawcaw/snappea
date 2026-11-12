---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do browser tabs share a rotating refresh token without tripping reuse detection?
  - What should a tab do when a refresh response is lost?
  - How do I model reuse detection so that it is tested, not just assumed?
---

# Token refresh across browser tabs

Two tabs of a web app share a refresh token in `localStorage`. The auth server
rotates refresh tokens: each refresh returns a new access and refresh token and
invalidates the old one. Presenting an already-rotated refresh token is treated as
theft: the whole token family is revoked and the user is logged out everywhere.

Each tab, when its access token expires, takes a cross-tab lock (Web Locks API),
re-reads storage, uses the stored token if another tab already refreshed, and
otherwise refreshes and stores the new pair before releasing the lock.

**Contract**

- The server accepts only the current refresh token, never a rotated-out one, and
  nothing after the family is revoked.
- A legitimate user with only these tabs is never logged out by reuse detection; the
  family is revoked only after an attacker presented a stolen token.
- A tab uses only a token the server issued, newer than the one that expired.
- Single flight: a token is refreshed only if a waiting tab holds it as its expired
  token. A tab never refreshes when a newer token is already stored.
- Progress (hot state): every tab that needs a fresh token gets one, unless a response
  was lost or the family was revoked; then it gives the session up.

**Atomic primitives**: the server's rotate-or-revoke on the family row; a storage
read or a write of the whole session (one JSON value); lock acquire and release.

## Lost responses (known limitation)

If a refresh response is lost, the server has rotated the token but the tab never
learns the new one. Retrying with the old token would be a reuse, so it would log the
user out (`tcBugRetryAfterLostResponse`). With a strict rotating server the protocol
cannot recover. The tab gives the session up instead: it writes a "lost" marker to
storage, so the other tab does not retry with the old token either, and the user logs
in again. The model checks that this never triggers reuse detection, and that giving
up happens only after a lost response or a revocation.

A server-side grace period (accepting the just-rotated token once more, returning the
same new pair) would let the tab retry. It also lets an attacker with a stolen token
use it during that window, and it would hide the missing lock in `tcBugNoLock`. It is
not modeled here.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `TokenSafety` and `TabsServed` (progress, single flight) |
| [`PSrc/AuthServer.p`](PSrc/AuthServer.p) | Rotation and reuse detection for one token family |
| [`PSrc/Browser.p`](PSrc/Browser.p) | Shared storage, the cross-tab lock, the lossy network (environment, marked as a driver) |
| [`PSrc/Tab.p`](PSrc/Tab.p) | The refresh algorithm |
| [`PSrc/BrokenTabs.p`](PSrc/BrokenTabs.p) | Negative controls: no lock; retry after a lost response |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, the `SecondTabReusedStoredToken` scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Result |
| --- | --- | --- | --- |
| `tcBothTabsExpire` | Both tabs' tokens expire at once | One refreshes; the other must take the stored token | passes, exhaustive |
| `tcLostRefreshResponse` | As above, and the network loses one response | Both tabs give the session up; no reuse | passes, exhaustive |
| `tcRepeatedExpiry` | Tab 1 expires twice, tab 2 once | The rotation chain continues from the stored token | passes, exhaustive |
| `tcStolenToken` | An attacker presents the login refresh token at any moment; tab 1 expires twice | Reuse detection works: the family is revoked, the server accepts nothing after, the tab ends logged out | passes, exhaustive |
| `tcBugNoLock` | Tabs that refresh without the lock | **Expected to fail**: `user logged out by reuse detection` | fails |
| `tcBugRetryAfterLostResponse` | A tab that retries with the same token after a timeout | **Expected to fail**: `user logged out by reuse detection` | fails |

## Run

```sh
snappea check tcBothTabsExpire tcLostRefreshResponse tcRepeatedExpiry tcStolenToken
snappea check mutation tcBothTabsExpire tcLostRefreshResponse tcRepeatedExpiry tcStolenToken
snappea check tcBugNoLock     # exits 1: user logged out by reuse detection
```

## Bounds and omissions

Two tabs, one or two expiries each, at most one lost response, one attacker request.
Not modeled: a tab closed while holding the lock (Web Locks releases it), lost
requests (they never reach the server, so a retry would be safe, but the tab cannot
tell them from lost responses), access-token expiry timing, several token families
(logins), and server-side grace periods.

Suppression: removing `goto Finished` after a tab gives up is equivalent (no event
reaches the tab afterwards).

## Testing the implementation

Emit `eRotated` and `eFamilyRevoked` from the auth server (or a stub of it),
`eTokenObtained` and `eSessionLost` from the tab code. To force the race, open two
tabs (or two instances of the refresh module over shared storage) and expire both
access tokens at once; to test the lost response, make the stub rotate the token and
then fail the request, and check that neither tab presents the old token again.
