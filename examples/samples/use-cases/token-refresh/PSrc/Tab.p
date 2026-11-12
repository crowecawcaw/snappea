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
      send locks, eAcquire, this;
    }
    on eGranted do {
      send storage, eLoad, this;
    }
    on eLoaded do (s: tSession) {
      if (s.lost) {
        GiveUp();
      } else if (s.token != token) {
        // Another tab refreshed while we waited for the lock.
        Use(s.token);
      } else {
        send network, eSendRefresh, (tab = this, refresh = s.token);
        goto Refreshing;
      }
    }
  }

  state Refreshing {
    on eRefreshResp do (r: (tab: machine, ok: bool, token: int)) {
      if (!r.ok) {
        // Revoked (someone presented a stolen token): the user is logged out.
        MarkLost();
      } else {
        fresh = r.token;
        send storage, eSave, (client = this, session = (token = fresh, lost = false));
        goto Storing;
      }
    }
    on eRefreshTimeout do { MarkLost(); }
  }

  state Storing {
    on eSaved do { Use(fresh); }
  }

  state Losing {
    on eSaved do { GiveUp(); }
  }

  state Finished { }

  fun Use(t: int) {
    token = t;
    announce eTokenObtained, (tab = id, token = t);
    send locks, eRelease;
    goto Locking;
  }

  fun MarkLost() {
    send storage, eSave, (client = this, session = (token = 0, lost = true));
    goto Losing;
  }

  fun GiveUp() {
    announce eSessionLost, (tab = id,);
    send locks, eRelease;
    // snappea-lint: allow mutation:remove-statement -- equivalent: Finished handles nothing, and neither does staying in Locking or Losing once no event is coming
    goto Finished;
  }
}
