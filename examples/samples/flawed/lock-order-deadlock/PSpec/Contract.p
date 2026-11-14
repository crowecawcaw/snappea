// The transfer contract, stated over events alone: the accounts the driver opened, the
// transfers it requested, and what the accounts did. It does not know in which order
// a worker takes locks, so it judges both designs by the same rule.

// The driver announces each account it opens and how many transfers it will request,
// then each transfer before starting it.
event AccountOpened: (account: int, balance: int);
event TransfersPlanned: int;
event TransferRequested: (transfer: int, src: int, dst: int, amount: int);
// An account announces when it grants its lock, applies a change, and releases.
event LockGranted: (account: int, transfer: int);
event Applied: (account: int, transfer: int, balance: int);
event LockReleased: (account: int, transfer: int);
// A worker announces that its transfer call returned.
event TransferDone: int;

// Safety: an account's lock has one holder at a time; a balance changes only under
// the lock of the transfer changing it; each transfer debits its source once and
// credits its destination once, and every balance equals what those changes predict.
spec TransfersIsolated observes AccountOpened, TransferRequested, LockGranted, Applied, LockReleased {
  var balances: map[int, int];
  // account -> transfer holding its lock
  var holder: map[int, int];
  var transfers: map[int, (src: int, dst: int, amount: int)];
  // transfer -> accounts it has already changed
  var changed: map[int, set[int]];

  start state Watching {
    on AccountOpened do (p: (account: int, balance: int)) {
      assert !(p.account in balances), format("account {0} opened twice", p.account);
      balances[p.account] = p.balance;
    }
    // snappea-lint: allow unused-payload-field -- accounts and amounts are checked by TransfersIsolated
    on TransferRequested do (p: (transfer: int, src: int, dst: int, amount: int)) {
      assert p.src in balances && p.dst in balances && p.src != p.dst,
        format("transfer {0} names accounts that were not opened", p.transfer);
      transfers[p.transfer] = (src = p.src, dst = p.dst, amount = p.amount);
      changed[p.transfer] = default(set[int]);
    }
    on LockGranted do (p: (account: int, transfer: int)) {
      assert !(p.account in holder),
        format("account {0} locked by transfer {1} while transfer {2} holds it",
          p.account, p.transfer, holder[p.account]);
      assert p.transfer in transfers, format("lock for unknown transfer {0}", p.transfer);
      assert p.account == transfers[p.transfer].src || p.account == transfers[p.transfer].dst,
        format("transfer {0} locked account {1}, which it does not touch", p.transfer, p.account);
      holder[p.account] = p.transfer;
    }
    // snappea-lint: allow unused-payload-field -- balances are checked by TransfersIsolated
    on Applied do (p: (account: int, transfer: int, balance: int)) {
      var t: (src: int, dst: int, amount: int);
      assert p.account in holder && holder[p.account] == p.transfer,
        format("transfer {0} changed account {1} without holding its lock", p.transfer, p.account);
      assert !(p.account in changed[p.transfer]),
        format("transfer {0} changed account {1} twice", p.transfer, p.account);
      changed[p.transfer] += (p.account);
      t = transfers[p.transfer];
      if (p.account == t.src) {
        balances[p.account] = balances[p.account] - t.amount;
      } else {
        balances[p.account] = balances[p.account] + t.amount;
      }
      assert p.balance == balances[p.account],
        format("account {0} has balance {1} after transfer {2}, expected {3}",
          p.account, p.balance, p.transfer, balances[p.account]);
    }
    on LockReleased do (p: (account: int, transfer: int)) {
      assert p.account in holder && holder[p.account] == p.transfer,
        format("transfer {0} released account {1}, which it does not hold", p.transfer, p.account);
      assert sizeof(changed[p.transfer]) == 2,
        format("transfer {0} released account {1} before applying both sides", p.transfer, p.account);
      holder -= (p.account);
    }
  }
}

// Progress: every requested transfer returns after both sides were applied. While any transfer is pending the spec is hot, so an
// execution that stops with transfers pending (a deadlock) fails as a liveness bug.
spec TransfersComplete observes TransfersPlanned, TransferRequested, Applied, TransferDone {
  // transfer -> sides applied so far
  var pending: map[int, int];
  var unfinished: int;

  start state Idle {
    on TransfersPlanned do (n: int) {
      assert n > 0, "planned no transfers";
      unfinished = n;
      goto TransferPending;
    }
    on TransferRequested do { assert false, "transfer requested without a plan"; }
    on Applied do { assert false, "change with no transfer pending"; }
    on TransferDone do { assert false, "transfer returned with none pending"; }
  }

  hot state TransferPending {
    on TransfersPlanned do { assert false, "transfers planned twice"; }

    // snappea-lint: allow unused-payload-field -- accounts and amounts are checked by TransfersIsolated
    on TransferRequested do (p: (transfer: int, src: int, dst: int, amount: int)) {
      pending[p.transfer] = 0;
    }

    // snappea-lint: allow unused-payload-field -- balances are checked by TransfersIsolated
    on Applied do (p: (account: int, transfer: int, balance: int)) {
      pending[p.transfer] = pending[p.transfer] + 1;
    }
    on TransferDone do (transfer: int) {
      assert transfer in pending, format("transfer {0} returned twice or was never requested", transfer);
      assert pending[transfer] == 2, format("transfer {0} returned before both sides were applied", transfer);
      pending -= (transfer);
      unfinished = unfinished - 1;
      if (unfinished == 0) { goto Idle; }
    }
  }
}
