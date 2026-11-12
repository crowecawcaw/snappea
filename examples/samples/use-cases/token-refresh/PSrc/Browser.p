// The browser side: storage shared by the tabs (localStorage), a cross-tab lock
// (the Web Locks API), and the network, which can lose a response.

// What the tabs share. `lost` marks a session a tab had to give up: the stored refresh
// token may already be rotated, so nobody may use it again.
type tSession = (token: int, lost: bool);

event eLoad: machine;
event eLoaded: tSession;
event eSave: (client: machine, session: tSession);
event eSaved;

machine Storage {
  var session: tSession;

  start state Serving {
    entry (initial: int) {
      session = (token = initial, lost = false);
    }
    on eLoad do (client: machine) {
      send client, eLoaded, session;
    }
    on eSave do (s: (client: machine, session: tSession)) {
      session = s.session;
      send s.client, eSaved;
    }
  }
}

event eAcquire: machine;
event eGranted;
event eRelease;

// navigator.locks.request("refresh", ...): one holder at a time, waiters in order.
machine LockManager {
  var waiting: seq[machine];

  start state Free {
    on eAcquire do (tab: machine) {
      send tab, eGranted;
      goto Held;
    }
  }

  state Held {
    on eAcquire do (tab: machine) {
      waiting += (sizeof(waiting), tab);
    }
    on eRelease do {
      if (sizeof(waiting) == 0) { goto Free; }
      send waiting[0], eGranted;
      waiting -= (0);
    }
  }
}

event eSendRefresh: (tab: machine, refresh: int);
// The request timed out: the tab cannot tell whether the server rotated the token.
event eRefreshTimeout;

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
    on eSendRefresh do (r: (tab: machine, refresh: int)) {
      send server, eRefresh, (client = this, tab = r.tab, refresh = r.refresh);
    }
    on eRefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (losses > 0 && $) {
        losses = losses - 1;
        announce eResponseLost;
        send r.tab, eRefreshTimeout;
        return;
      }
      send r.tab, eRefreshResp, r;
    }
  }
}
