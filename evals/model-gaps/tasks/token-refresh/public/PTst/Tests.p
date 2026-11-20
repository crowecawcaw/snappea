// Drivers: they log the user in (token 1 in storage and at the server), open the tabs
// and announce how often each tab's access token expires.

type Browser = (server: machine, storage: machine, locks: machine, network: machine);

fun LogIn(losses: int): Browser {
  var server: machine;
  announce LoggedIn, 1;
  server = new AuthServer(1);
  return (server = server, storage = new Storage(1), locks = new LockManager(),
          network = new Network((server = server, losses = losses)));
}

fun OpenTab(w: Browser, id: int, rounds: int) {
  announce TabPlanned, (tab = id, rounds = rounds);
  new Tab((storage = w.storage, locks = w.locks, network = w.network, id = id, rounds = rounds, token = 1));
}

// Both tabs' access tokens expire at the same moment.
machine BothTabsExpire {
  start state Run {
    entry {
      var w: Browser;
      w = LogIn(0);
      OpenTab(w, 1, 1);
      OpenTab(w, 2, 1);
    }
  }
}

// A tab's token expires twice: the second refresh starts from the token the first stored.
machine RepeatedExpiry {
  start state Run {
    entry {
      var w: Browser;
      w = LogIn(0);
      OpenTab(w, 1, 2);
    }
  }
}

// The response to a refresh may be lost: the tab gives the session up.
machine LostResponse {
  start state Run {
    entry {
      var w: Browser;
      w = LogIn(1);
      OpenTab(w, 1, 1);
    }
  }
}

// The refresh token was stolen at login. The attacker presents it at a moment the
// scheduler picks, before or after the tab's refreshes.
machine StolenToken {
  start state Run {
    entry {
      var w: Browser;
      w = LogIn(0);
      OpenTab(w, 1, 2);
      announce TokenStolen, 1;
      send w.server, Refresh, (client = this, tab = this, refresh = 1);
    }
    ignore RefreshResp;
  }
}

test tcBothTabsExpire [main=BothTabsExpire]:
  assert TokenSafety, TabsServed in { BothTabsExpire, AuthServer, Storage, LockManager, Network, Tab };

test tcRepeatedExpiry [main=RepeatedExpiry]:
  assert TokenSafety, TabsServed in { RepeatedExpiry, AuthServer, Storage, LockManager, Network, Tab };

test tcLostResponse [main=LostResponse]:
  assert TokenSafety, TabsServed in { LostResponse, AuthServer, Storage, LockManager, Network, Tab };

test tcStolenToken [main=StolenToken]:
  assert TokenSafety, TabsServed in { StolenToken, AuthServer, Storage, LockManager, Network, Tab };
