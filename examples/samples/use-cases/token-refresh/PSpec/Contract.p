// The token contract, over events the auth server announces when it rotates or revokes,
// events tabs announce when they get a token or give a session up, and the network's
// announcement that it lost a response. The specs rebuild the token chain from that
// history; they never read the server or the storage.

event eRotated: (from: int, next: int);
event eFamilyRevoked: (refresh: int);
event eTokenObtained: (tab: int, token: int);
event eSessionLost: (tab: int);
event eResponseLost;
// From the driver, before anything starts: the token everyone holds at login.
event eLoggedIn: int;
// From the driver: an attacker presents a copy of this refresh token.
event eTokenStolen: int;

// Safety.
// - The server accepts only the current refresh token, never a rotated-out one.
// - A legitimate user with these two tabs is never logged out by reuse detection:
//   the family is revoked only after an attacker presented a stolen token, and after
//   that the server accepts no token of the family.
// - A tab only uses a token the server issued, and newer than the one that expired.
spec TokenSafety observes eLoggedIn, eTokenStolen, eRotated, eFamilyRevoked, eTokenObtained {
  var current: int;
  var stolen: bool;
  var revoked: bool;
  var login: int;
  // tab -> the newest token it has used
  var held: map[int, int];

  start state Watching {
    on eLoggedIn do (t: int) {
      current = t;
      login = t;
    }
    on eTokenStolen do (t: int) {
      assert t <= current, format("attacker stole token {0}, which was never issued", t);
      stolen = true;
    }
    on eRotated do (p: (from: int, next: int)) {
      assert !revoked, format("server accepted refresh token {0} after the family was revoked", p.from);
      assert p.from == current, format("server accepted rotated-out refresh token {0}; the current one is {1}", p.from, current);
      assert p.next > current, format("server issued token {0}, which is not newer than {1}", p.next, current);
      current = p.next;
    }
    on eFamilyRevoked do (p: (refresh: int)) {
      assert stolen, format("user logged out by reuse detection: refresh token {0} was presented after it was rotated", p.refresh);
      assert p.refresh < current, format("family revoked over refresh token {0}, which was not rotated out", p.refresh);
      revoked = true;
    }
    on eTokenObtained do (p: (tab: int, token: int)) {
      assert p.token <= current, format("tab {0} used token {1}, which the server never issued", p.tab, p.token);
      if (!(p.tab in held)) { held[p.tab] = login; }
      assert p.token > held[p.tab],
        format("tab {0} got token {1}, which is not newer than its expired token {2}", p.tab, p.token, held[p.tab]);
      held[p.tab] = p.token;
    }
  }
}

// Progress: every tab that needs a fresh access token eventually gets one. The only
// excuses are a lost response (see README.md) and a revoked family: then the tab
// gives the session up, and the user logs in again. The driver announces each tab and how many times its
// token will expire.
event eTabPlanned: (tab: int, rounds: int);

spec TabsServed observes eTabPlanned, eTokenObtained, eSessionLost, eResponseLost, eFamilyRevoked {
  // tab -> rounds still waiting for a token
  var waiting: map[int, int];
  // a response was lost, or the family revoked
  var excuse: bool;

  start state Idle {
    on eTabPlanned do (p: (tab: int, rounds: int)) { Plan(p.tab, p.rounds); }
    on eTokenObtained do { assert false, "token obtained by a tab that needs none"; }
    on eSessionLost do { assert false, "session given up by a tab that needs no token"; }
    on eResponseLost do { assert false, "refresh while no tab needs a token"; }
    // An attacker can present the stolen token after the tabs are done.
    // snappea-lint: allow unused-payload-field -- TokenSafety checks which token caused it
    on eFamilyRevoked do (p: (refresh: int)) { excuse = true; }
  }

  hot state Busy {
    on eTabPlanned do (p: (tab: int, rounds: int)) { Plan(p.tab, p.rounds); }
    // snappea-lint: allow unused-payload-field -- TokenSafety checks the token; this spec counts rounds
    on eTokenObtained do (p: (tab: int, token: int)) {
      assert p.tab in waiting && waiting[p.tab] > 0, format("tab {0} got a token it did not need", p.tab);
      waiting[p.tab] = waiting[p.tab] - 1;
      Check();
    }
    on eSessionLost do (p: (tab: int)) {
      assert excuse, format("tab {0} gave its session up, but no response was lost and nothing was revoked", p.tab);
      assert p.tab in waiting && waiting[p.tab] > 0, format("tab {0} gave up a session it did not need", p.tab);
      waiting[p.tab] = 0;
      Check();
    }
    on eResponseLost do { excuse = true; }
    // snappea-lint: allow unused-payload-field -- TokenSafety checks which token caused it
    on eFamilyRevoked do (p: (refresh: int)) { excuse = true; }
  }

  fun Plan(tab: int, rounds: int) {
    assert !(tab in waiting) && rounds > 0, format("tab {0} planned twice or with no work", tab);
    waiting[tab] = rounds;
    goto Busy;
  }

  fun Check() {
    var tab: int;
    foreach (tab in keys(waiting)) {
      if (waiting[tab] > 0) { return; }
    }
    goto Idle;
  }
}
