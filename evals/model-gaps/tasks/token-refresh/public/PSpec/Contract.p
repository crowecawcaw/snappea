// The token contract, over events the auth server announces when it rotates or revokes,
// events tabs announce when they get a token or give a session up, and what the
// drivers and the network announce. The specs rebuild the token chain from that
// history; they never read the server or the storage.

// From the network: a request's timer fired before its response reached the tab.
event ResponseLost;
// From the driver, before anything starts: the token everyone holds at login.
event LoggedIn: int;
// From the driver: an attacker has a copy of this refresh token and will present it.
event TokenStolen: int;
// From the driver: a tab and how many times its token will expire.
event TabPlanned: (tab: int, rounds: int);

// Safety.
// - The server accepts only the current refresh token, never a rotated-out one, and
//   revokes the family only over a rotated-out one.
// - A tab only uses a token the server issued, and newer than the one that expired.
spec TokenSafety observes LoggedIn, TokenStolen, Rotated, FamilyRevoked, TokenObtained {
  var current: int;
  var login: int;
  // tab -> the newest token it has used
  var held: map[int, int];

  start state Watching {
    on LoggedIn do (t: int) {
      current = t;
      login = t;
    }
    on TokenStolen do (t: int) {
      assert t <= current, format("attacker stole token {0}, which was never issued", t);
    }
    on Rotated do (p: (from: int, next: int)) {
      assert p.from == current, format("server accepted rotated-out refresh token {0}; the current one is {1}", p.from, current);
      assert p.next > current, format("server issued token {0}, which is not newer than {1}", p.next, current);
      current = p.next;
    }
    on FamilyRevoked do (p: (refresh: int)) {
      assert p.refresh < current, format("family revoked over refresh token {0}, which was not rotated out", p.refresh);
    }
    on TokenObtained do (p: (tab: int, token: int)) {
      assert p.token <= current, format("tab {0} used token {1}, which the server never issued", p.tab, p.token);
      if (!(p.tab in held)) { held[p.tab] = login; }
      assert p.token > held[p.tab],
        format("tab {0} got token {1}, which is not newer than its expired token {2}", p.tab, p.token, held[p.tab]);
      held[p.tab] = p.token;
    }
  }
}

// Progress: every tab that needs a fresh access token eventually gets one. The only
// excuses are a response that missed its timer and a revoked family: then the tab
// gives the session up, and the user logs in again.
// Single flight: a token is refreshed only if a tab still waiting holds it as its
// expired token. A tab that refreshes although a newer token is already stored
// wastes a rotation.
spec TabsServed observes LoggedIn, TabPlanned, TokenObtained, SessionLost, ResponseLost, FamilyRevoked,
    Rotated {
  // tab -> rounds still waiting for a token
  var waiting: map[int, int];
  // tab -> the token it holds (expired while it is waiting)
  var held: map[int, int];
  var login: int;
  // a response was lost, or the family revoked
  var excuse: bool;

  start state Idle {
    on LoggedIn do (t: int) { login = t; }
    on TabPlanned do (p: (tab: int, rounds: int)) { Plan(p.tab, p.rounds); }
    on Rotated do { assert false, "refresh while no tab needs a token"; }
    on TokenObtained do { assert false, "token obtained by a tab that needs none"; }
    on SessionLost do { assert false, "session given up by a tab that needs no token"; }
    on ResponseLost do { assert false, "refresh while no tab needs a token"; }
    // An attacker can present the stolen token after the tabs are done.
    on FamilyRevoked do (p: (refresh: int)) { excuse = true; }
  }

  hot state Busy {
    on LoggedIn do { assert false, "logged in twice"; }
    on TabPlanned do (p: (tab: int, rounds: int)) { Plan(p.tab, p.rounds); }
    on TokenObtained do (p: (tab: int, token: int)) {
      assert p.tab in waiting && waiting[p.tab] > 0, format("tab {0} got a token it did not need", p.tab);
      waiting[p.tab] = waiting[p.tab] - 1;
      held[p.tab] = p.token;
      Check();
    }
    on SessionLost do (p: (tab: int)) {
      assert excuse, format("tab {0} gave its session up, but no response was lost and nothing was revoked", p.tab);
      assert p.tab in waiting && waiting[p.tab] > 0, format("tab {0} gave up a session it did not need", p.tab);
      waiting[p.tab] = 0;
      Check();
    }
    on ResponseLost do { excuse = true; }
    on Rotated do (p: (from: int, next: int)) {
      var tab: int;
      var justified: bool;
      foreach (tab in keys(waiting)) {
        if (waiting[tab] > 0 && held[tab] == p.from) { justified = true; }
      }
      assert justified, format("token {0} was refreshed, but no waiting tab held it: a tab ignored the newer stored token", p.from);
    }
    on FamilyRevoked do (p: (refresh: int)) { excuse = true; }
  }

  fun Plan(tab: int, rounds: int) {
    assert !(tab in waiting) && rounds > 0, format("tab {0} planned twice or with no work", tab);
    waiting[tab] = rounds;
    held[tab] = login;
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
