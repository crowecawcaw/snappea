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

// The network between the tabs and the auth server, and the request timers. Requests
// always reach the server. Up to `losses` responses are lost: the request's timer
// fires and the tab gets RefreshTimeout instead of the response.
machine Network {
  var server: machine;
  var losses: int;
  // tab -> the refresh token of its request in flight
  var presented: map[machine, int];

  start state Up {
    entry (input: (server: machine, losses: int)) {
      server = input.server;
      losses = input.losses;
    }
    on SendRefresh do (r: (tab: machine, refresh: int)) {
      presented[r.tab] = r.refresh;
      send server, Refresh, (client = this, tab = r.tab, refresh = r.refresh);
    }
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (losses > 0 && $) {
        losses = losses - 1;
        announce ResponseLost;
        send r.tab, RefreshTimeout, (refresh = presented[r.tab],);
        return;
      }
      send r.tab, RefreshResp, r;
    }
  }
}
