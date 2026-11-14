// Two plausible broken tabs, kept as negative controls.

// No cross-tab lock: each tab reads the shared refresh token and refreshes on its own.
// When both tokens expire together, both present the same refresh token; the second
// is a reuse, and the server logs the user out. tcBugNoLock shows it.
machine UnlockedTab {
  var storage: Storage;
  var network: machine;
  var id: int;
  var fresh: int;

  start state Loading {
    entry (input: (storage: Storage, network: machine, id: int)) {
      storage = input.storage;
      network = input.network;
      id = input.id;
      send storage, Load, this;
    }
    on Loaded do (s: Session) {
      send network, SendRefresh, (tab = this, refresh = s.token);
    }
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      fresh = r.token;
      send storage, Save, (client = this, session = (token = fresh, lost = false));
    }
    on Saved do {
      announce TokenObtained, (tab = id, token = fresh);
    }
  }
}

// Retries a timed-out refresh with the same refresh token. If the request reached the
// server, that token was rotated, so the retry is a reuse and the user is logged out.
// tcBugRetryAfterLostResponse shows it: this is why Tab gives the session up instead.
machine RetryingTab {
  var storage: Storage;
  var locks: LockManager;
  var network: machine;
  var id: int;
  var refresh: int;
  var fresh: int;

  start state Locking {
    entry (input: (storage: Storage, locks: LockManager, network: machine, id: int)) {
      storage = input.storage;
      locks = input.locks;
      network = input.network;
      id = input.id;
      send locks, Acquire, this;
    }
    on Granted do {
      send storage, Load, this;
    }
    on Loaded do (s: Session) {
      refresh = s.token;
      send network, SendRefresh, (tab = this, refresh = refresh);
    }
    on RefreshTimeout do {
      // BUG: the server may already have rotated this token.
      send network, SendRefresh, (tab = this, refresh = refresh);
    }
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      fresh = r.token;
      send storage, Save, (client = this, session = (token = fresh, lost = false));
    }
    on Saved do {
      announce TokenObtained, (tab = id, token = fresh);
      send locks, Release;
    }
  }
}
