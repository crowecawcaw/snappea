// A browser tab. `rounds` is how many times its access token expires; each time it
// needs a fresh one. Tabs share the session in storage and take the cross-tab lock to
// refresh, so that one refresh serves every tab whose token expired.

machine Tab {
  var storage: machine;
  var locks: machine;
  var network: machine;
  var id: int;
  var rounds: int;
  // the token this tab holds; at the start of a round it has expired
  var token: int;
  var fresh: int;

  start state Init {
    entry (input: (storage: machine, locks: machine, network: machine, id: int, rounds: int, token: int)) {
      storage = input.storage;
      locks = input.locks;
      network = input.network;
      id = input.id;
      rounds = input.rounds;
      token = input.token;
      goto Checking;
    }
  }

  // A round starts here: most of the time another tab has refreshed already and the
  // lock is not needed.
  state Checking {
    entry {
      if (rounds == 0) { goto Finished; }
      rounds = rounds - 1;
      send storage, Load, this;
    }
    on Loaded do (s: Session) {
      if (s.lost) {
        announce SessionLost, (tab = id,);
        goto Finished;
      }
      if (s.token != token) {
        Obtain(s.token);
        goto Checking;
      }
      send locks, Acquire, this;
      goto Locking;
    }
    ignore RefreshTimeout;
  }

  state Locking {
    on Granted do {
      send storage, Load, this;
    }
    on Loaded do (s: Session) {
      if (s.lost) {
        send locks, Release;
        announce SessionLost, (tab = id,);
        goto Finished;
      }
      if (s.token != token) {
        Obtain(s.token);
        send locks, Release;
        goto Checking;
      }
      send network, SendRefresh, (tab = this, refresh = token);
      goto Refreshing;
    }
    ignore RefreshTimeout;
  }

  state Refreshing {
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (r.ok) {
        fresh = r.token;
        send storage, Save, (client = this, session = (token = fresh, lost = false));
        goto Storing;
      }
      GiveUp();
    }
    on RefreshTimeout do (t: (refresh: int)) {
      // The server may or may not have rotated the token: it cannot be presented again.
      if (t.refresh == token) { GiveUp(); }
    }
  }

  state Storing {
    on Saved do {
      Obtain(fresh);
      send locks, Release;
      goto Checking;
    }
    ignore RefreshTimeout;
  }

  state Losing {
    on Saved do {
      send locks, Release;
      goto Finished;
    }
    ignore RefreshResp, RefreshTimeout;
  }

  state Finished {
    ignore RefreshResp, RefreshTimeout;
  }

  fun Obtain(t: int) {
    token = t;
    announce TokenObtained, (tab = id, token = t);
  }

  // The session is over for every tab: say so in storage, so no tab uses the stored
  // refresh token again.
  fun GiveUp() {
    announce SessionLost, (tab = id,);
    send storage, Save, (client = this, session = (token = token, lost = true));
    goto Losing;
  }
}
