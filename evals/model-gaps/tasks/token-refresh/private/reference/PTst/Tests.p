// Drivers: they log the user in (token 1 in storage and at the server), open the tabs
// and announce how often each tab's access token expires.

type Browser = (server: machine, storage: machine, locks: machine, network: machine);

fun LogIn(late: int, stale: int): Browser {
  var server: machine;
  announce LoggedIn, 1;
  server = new AuthServer(1);
  return (server = server, storage = new Storage(1), locks = new LockManager(),
          network = new Network((server = server, late = late, stale = stale)));
}

fun OpenTab(w: Browser, id: int, rounds: int) {
  announce TabPlanned, (tab = id, rounds = rounds);
  new Tab((storage = w.storage, locks = w.locks, network = w.network, id = id, rounds = rounds, token = 1));
}

// tabs[i] is how many times tab i + 1's token expires. With `attacker`, a copy of the
// login refresh token is presented to the server at a moment the scheduler picks.
type Plan = (tabs: seq[int], late: int, stale: int, attacker: bool);

machine Session1 {
  start state Run {
    entry (p: Plan) {
      var w: Browser;
      var i: int;
      w = LogIn(p.late, p.stale);
      while (i < sizeof(p.tabs)) {
        OpenTab(w, i + 1, p.tabs[i]);
        i = i + 1;
      }
      if (p.attacker) {
        announce TokenStolen, 1;
        send w.server, Refresh, (client = this, tab = this, refresh = 1);
      }
    }
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (!r.ok) { announce AttackRefused, 1; }
    }
  }
}

fun Rounds1(a: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  return s;
}

fun Rounds2(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  return s;
}

fun Rounds3(a: int, b: int, c: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  s += (2, c);
  return s;
}

machine BothTabsExpire { start state Run { entry { new Session1((tabs = Rounds2(1, 1), late = 0, stale = 0, attacker = false)); } } }
machine RepeatedExpiry { start state Run { entry { new Session1((tabs = Rounds2(2, 1), late = 0, stale = 0, attacker = false)); } } }
machine ThreeTabs { start state Run { entry { new Session1((tabs = Rounds3(1, 1, 1), late = 0, stale = 0, attacker = false)); } } }
machine LostResponse { start state Run { entry { new Session1((tabs = Rounds2(1, 1), late = 1, stale = 0, attacker = false)); } } }
machine LostResponseRepeated { start state Run { entry { new Session1((tabs = Rounds2(2, 2), late = 1, stale = 0, attacker = false)); } } }
machine LostResponseThreeTabs { start state Run { entry { new Session1((tabs = Rounds3(1, 1, 1), late = 1, stale = 0, attacker = false)); } } }
machine StaleTimer { start state Run { entry { new Session1((tabs = Rounds1(3), late = 0, stale = 1, attacker = false)); } } }
machine StaleTimerTwoTabs { start state Run { entry { new Session1((tabs = Rounds2(2, 2), late = 0, stale = 1, attacker = false)); } } }
machine StolenToken { start state Run { entry { new Session1((tabs = Rounds1(3), late = 0, stale = 0, attacker = true)); } } }
machine StolenTokenTwoTabs { start state Run { entry { new Session1((tabs = Rounds2(2, 1), late = 0, stale = 0, attacker = true)); } } }
machine StolenTokenLostResponse { start state Run { entry { new Session1((tabs = Rounds2(2, 1), late = 1, stale = 0, attacker = true)); } } }

test tcBothTabsExpire [main=BothTabsExpire]:
  assert TokenSafety, TabsServed, ReuseRevokes in { BothTabsExpire, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcRepeatedExpiry [main=RepeatedExpiry]:
  assert TokenSafety, TabsServed, ReuseRevokes in { RepeatedExpiry, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcThreeTabs [main=ThreeTabs]:
  assert TokenSafety, TabsServed, ReuseRevokes in { ThreeTabs, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcLostResponse [main=LostResponse]:
  assert TokenSafety, TabsServed, ReuseRevokes in { LostResponse, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcLostResponseRepeated [main=LostResponseRepeated]:
  assert TokenSafety, TabsServed, ReuseRevokes in { LostResponseRepeated, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcLostResponseThreeTabs [main=LostResponseThreeTabs]:
  assert TokenSafety, TabsServed, ReuseRevokes in { LostResponseThreeTabs, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcStaleTimer [main=StaleTimer]:
  assert TokenSafety, TabsServed, ReuseRevokes in { StaleTimer, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcStaleTimerTwoTabs [main=StaleTimerTwoTabs]:
  assert TokenSafety, TabsServed, ReuseRevokes in { StaleTimerTwoTabs, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcStolenToken [main=StolenToken]:
  assert TokenSafety, TabsServed, ReuseRevokes in { StolenToken, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcStolenTokenTwoTabs [main=StolenTokenTwoTabs]:
  assert TokenSafety, TabsServed, ReuseRevokes in { StolenTokenTwoTabs, Session1, AuthServer, Storage, LockManager, Network, Tab };
test tcStolenTokenLostResponse [main=StolenTokenLostResponse]:
  assert TokenSafety, TabsServed, ReuseRevokes in { StolenTokenLostResponse, Session1, AuthServer, Storage, LockManager, Network, Tab };
