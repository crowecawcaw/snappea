// A browser tab. Each round, its access token has expired and it needs a fresh one:
//
//   take the cross-tab lock
//   re-read the shared storage   (another tab may have refreshed meanwhile)
//   if the stored token is newer than ours: use it, done
//   else refresh with the stored refresh token, store the new pair, done
//   release the lock
//
// A lost response leaves the tab not knowing whether the server rotated the token. A
// retry with the old token would look like theft if it did, so the tab gives the
// session up instead: it marks the stored session lost (so the other tab does not
// retry either) and the user has to log in again. See README.md, "Lost responses".

machine Tab {
  var storage: Storage;
  var locks: LockManager;
  var network: machine;
  var id: int;
  var rounds: int;
  // the access token this tab holds; at the start of a round it has expired
  var token: int;
  var fresh: int;

  start state Init {
    entry (input: (storage: Storage, locks: LockManager, network: machine, id: int, rounds: int, token: int)) {
      storage = input.storage;
      locks = input.locks;
      network = input.network;
      id = input.id;
      rounds = input.rounds;
      token = input.token;
      goto Locking;
    }
  }

  state Locking {
    entry {
      if (rounds == 0) { goto Finished; }
      rounds = rounds - 1;
      send locks, Acquire, this;
    }
    on Granted do {
      send storage, Load, this;
    }
    on Loaded do (s: Session) {
      if (s.lost) {
        GiveUp();
      } else if (s.token != token) {
        // Another tab refreshed while we waited for the lock.
        Use(s.token);
      } else {
        send network, SendRefresh, (tab = this, refresh = s.token);
        goto Refreshing;
      }
    }
  }

  state Refreshing {
    on RefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      assert r.tab == this, "refresh response belongs to another tab";
      if (!r.ok) {
        // Revoked (someone presented a stolen token): the user is logged out.
        MarkLost();
      } else {
        fresh = r.token;
        send storage, Save, (client = this, session = (token = fresh, lost = false));
        goto Storing;
      }
    }
    on RefreshTimeout do { MarkLost(); }
  }

  state Storing {
    on Saved do { Use(fresh); }
  }

  state Losing {
    on Saved do { GiveUp(); }
  }

  state Finished { }

  fun Use(t: int) {
    token = t;
    announce TokenObtained, (tab = id, token = t);
    send locks, Release;
    goto Locking;
  }

  fun MarkLost() {
    send storage, Save, (client = this, session = (token = 0, lost = true));
    goto Losing;
  }

  fun GiveUp() {
    announce SessionLost, (tab = id,);
    send locks, Release;
    // snappea-lint: allow mutation:remove-statement -- equivalent: Finished handles nothing, and neither does staying in Locking or Losing once no event is coming
    goto Finished;
  }
}
