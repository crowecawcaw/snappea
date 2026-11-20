// The browser platform and the network: the environment of the design.

machine Storage {
  var session: Session;

  start state Serving {
    entry (initial: int) {
      session = (token = initial, lost = false);
    }
    on Load do (client: machine) {
      send client, Loaded, session;
    }
    on Save do (s: (client: machine, session: Session)) {
      session = s.session;
      send s.client, Saved;
    }
  }
}

// navigator.locks.request("refresh", ...): one holder at a time, waiters in order.
machine LockManager {
  var waiting: seq[machine];

  start state Free {
    on Acquire do (tab: machine) {
      send tab, Granted;
      goto Held;
    }
  }

  state Held {
    on Acquire do (tab: machine) {
      waiting += (sizeof(waiting), tab);
    }
    on Release do {
      if (sizeof(waiting) == 0) { goto Free; }
      send waiting[0], Granted;
      waiting -= (0);
    }
  }
}

event TimerFired: (tab: machine, refresh: int);

// The network between the tabs and the auth server, and the request timers. Requests
// always reach the server. For up to `late` requests the timer fires first: the tab
// gets RefreshTimeout, and the response either never arrives or arrives afterwards.
// For up to `stale` requests the response arrives in time but the timer is not
// cancelled: it fires at some later moment.
machine Network {
  var server: machine;
  var late: int;
  var stale: int;
  // tab -> the refresh token of its request in flight
  var presented: map[machine, int];

  start state Up {
    entry (input: (server: machine, late: int, stale: int)) {
      server = input.server;
      late = input.late;
      stale = input.stale;
    }
    on SendRefresh do (r: (tab: machine, refresh: int)) {
      presented[r.tab] = r.refresh;
      send server, Refresh, (client = this, tab = r.tab, refresh = r.refresh);
    }
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (late > 0 && $) {
        late = late - 1;
        announce ResponseLost;
        send r.tab, RefreshTimeout, (refresh = presented[r.tab],);
        if ($) { send r.tab, RefreshResp, r; }
        return;
      }
      send r.tab, RefreshResp, r;
      if (stale > 0 && $) {
        stale = stale - 1;
        send this, TimerFired, (tab = r.tab, refresh = presented[r.tab]);
      }
    }
    on TimerFired do (t: (tab: machine, refresh: int)) {
      send t.tab, RefreshTimeout, (refresh = t.refresh,);
    }
  }
}
