// The browser side: storage shared by the tabs (localStorage), a cross-tab lock
// (the Web Locks API), and the network, which can lose a response.

// What the tabs share. `lost` marks a session a tab had to give up: the stored refresh
// token may already be rotated, so nobody may use it again.
type Session = (token: int, lost: bool);

event Load: machine;
event Loaded: Session;
event Save: (client: machine, session: Session);
event Saved;

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

event Acquire: machine;
event Granted;
event Release;

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

event SendRefresh: (tab: machine, refresh: int);
// The request timed out: the tab cannot tell whether the server rotated the token.
event RefreshTimeout;

// The network between the tabs and the auth server. It loses up to `losses` responses:
// the server has rotated the token, but the tab never learns the new one. This is the
// environment, not the design, so it is marked as a driver.
// snappea: driver
machine Network {
  var server: AuthServer;
  var losses: int;

  start state Up {
    entry (input: (server: AuthServer, losses: int)) {
      server = input.server;
      losses = input.losses;
    }
    on SendRefresh do (r: (tab: machine, refresh: int)) {
      send server, Refresh, (client = this, tab = r.tab, refresh = r.refresh);
    }
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (losses > 0 && $) {
        losses = losses - 1;
        announce ResponseLost;
        send r.tab, RefreshTimeout;
        return;
      }
      send r.tab, RefreshResp, r;
    }
  }
}
