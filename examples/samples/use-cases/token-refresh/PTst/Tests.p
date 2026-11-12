// Drivers: they log the user in (token 1 in storage and at the server), start the
// tabs, and announce how often each tab's access token expires. Tabs, storage, the
// lock and the server are the design under test; the network is the environment.

type tWorld = (server: AuthServer, storage: Storage, locks: LockManager, network: Network);

fun LogIn(losses: int): tWorld {
  var server: AuthServer;
  announce eLoggedIn, 1;
  server = new AuthServer(1);
  return (server = server, storage = new Storage(1), locks = new LockManager(),
          network = new Network((server = server, losses = losses)));
}

fun OpenTab(w: tWorld, id: int, rounds: int) {
  announce eTabPlanned, (tab = id, rounds = rounds);
  new Tab((storage = w.storage, locks = w.locks, network = w.network, id = id, rounds = rounds, token = 1));
}

// Both tabs' access tokens expire at the same moment: one refreshes, the other must
// pick up the new token from storage instead of refreshing with the old one.
machine BothTabsExpire {
  start state Run {
    entry {
      var w: tWorld;
      w = LogIn(0);
      OpenTab(w, 1, 1);
      OpenTab(w, 2, 1);
    }
  }
}

// The response to a refresh is lost. The tabs must not present the rotated token
// again: both end with the session given up, and reuse detection never fires.
machine LostRefreshResponse {
  start state Run {
    entry {
      var w: tWorld;
      w = LogIn(1);
      OpenTab(w, 1, 1);
      OpenTab(w, 2, 1);
    }
  }
}

// Tab 1's token expires twice, tab 2's once: the second round must start from the
// token the first round stored, while tab 2 may refresh in between.
machine RepeatedExpiry {
  start state Run {
    entry {
      var w: tWorld;
      w = LogIn(0);
      OpenTab(w, 1, 2);
      OpenTab(w, 2, 1);
    }
  }
}

// The refresh token was stolen at login. The attacker presents it at a moment the
// scheduler picks: before or after the tab rotates it, the second presentation is a
// reuse, the family is revoked, and the tab ends logged out, even with a token the
// server issued to it (its second round). This is reuse detection doing its job, and
// the reason the tabs must never trigger it themselves.
machine StolenToken {
  start state Run {
    entry {
      var w: tWorld;
      w = LogIn(0);
      OpenTab(w, 1, 2);
      announce eTokenStolen, 1;
      send w.server, eRefresh, (client = this, tab = this, refresh = 1);
    }
    ignore eRefreshResp;
  }
}

machine BugNoLock {
  start state Run {
    entry {
      var w: tWorld;
      w = LogIn(0);
      announce eTabPlanned, (tab = 1, rounds = 1);
      announce eTabPlanned, (tab = 2, rounds = 1);
      new UnlockedTab((storage = w.storage, network = w.network, id = 1));
      new UnlockedTab((storage = w.storage, network = w.network, id = 2));
    }
  }
}

machine BugRetryAfterLostResponse {
  start state Run {
    entry {
      var w: tWorld;
      w = LogIn(1);
      announce eTabPlanned, (tab = 1, rounds = 1);
      new RetryingTab((storage = w.storage, locks = w.locks, network = w.network, id = 1));
    }
  }
}

// Covered: one tab refreshed while the other waited for the lock, and the waiting tab
// took the stored token instead of refreshing.
scenario SecondTabReusedStoredToken observes eRotated, eTokenObtained {
  var issued: int;
  start hot state Waiting {
    on eRotated do (p: (from: int, next: int)) { issued = p.next; }
    on eTokenObtained do (p: (tab: int, token: int)) {
      if (p.token == issued) { goto OneUsed; }
    }
  }
  hot state OneUsed {
    on eTokenObtained do (p: (tab: int, token: int)) {
      if (p.token == issued) { goto Covered; }
    }
    ignore eRotated;
  }
  cold state Covered {
    ignore eRotated, eTokenObtained;
  }
}

test tcBothTabsExpire [main=BothTabsExpire]:
  assert TokenSafety, TabsServed in { BothTabsExpire, AuthServer, Storage, LockManager, Network, Tab };

test tcLostRefreshResponse [main=LostRefreshResponse]:
  assert TokenSafety, TabsServed in { LostRefreshResponse, AuthServer, Storage, LockManager, Network, Tab };

test tcStolenToken [main=StolenToken]:
  assert TokenSafety, TabsServed in { StolenToken, AuthServer, Storage, LockManager, Network, Tab };

test tcRepeatedExpiry [main=RepeatedExpiry]:
  assert TokenSafety, TabsServed in { RepeatedExpiry, AuthServer, Storage, LockManager, Network, Tab };

// expect-bug: user logged out by reuse detection
test tcBugNoLock [main=BugNoLock]:
  assert TokenSafety, TabsServed in { BugNoLock, AuthServer, Storage, LockManager, Network, UnlockedTab };

// expect-bug: user logged out by reuse detection
test tcBugRetryAfterLostResponse [main=BugRetryAfterLostResponse]:
  assert TokenSafety, TabsServed in { BugRetryAfterLostResponse, AuthServer, Storage, LockManager, Network, RetryingTab };
