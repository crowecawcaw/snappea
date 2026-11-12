// A database with row locks, as in SELECT ... FOR UPDATE. A transfer runs as one
// transaction: lock the rows it needs, one statement at a time; then commit, which
// applies both balance changes and releases both locks. A lock request on a row
// another transaction holds waits until that transaction commits.
//
// Real primitives (each atomic): lock(row), commit(transaction). Between the two lock
// statements of one transaction, other transactions run.

event eLock: (worker: machine, transfer: int, account: int);
event eLocked: (account: int);
event eCommit: (worker: machine, transfer: tTransfer);
event eCommitted;

type tWaiter = (worker: machine, transfer: int, account: int);
type tTransfer = (transfer: int, src: int, dst: int, amount: int);

machine Database {
  var balances: map[int, int];
  // account -> transaction holding its row lock
  var holder: map[int, int];
  // account -> lock requests waiting for it, in arrival order
  var waiters: map[int, seq[tWaiter]];

  start state Serving {
    entry (accounts: map[int, int]) {
      var account: int;
      balances = accounts;
      foreach (account in keys(balances)) {
        waiters[account] = default(seq[tWaiter]);
      }
    }
    on eLock do (w: tWaiter) {
      if (w.account in holder) {
        waiters[w.account] += (sizeof(waiters[w.account]), w);
        return;
      }
      Grant(w);
    }
    on eCommit do (c: (worker: machine, transfer: tTransfer)) {
      Apply(c.transfer.src, c.transfer.transfer, -c.transfer.amount);
      Apply(c.transfer.dst, c.transfer.transfer, c.transfer.amount);
      Release(c.transfer.src, c.transfer.transfer);
      Release(c.transfer.dst, c.transfer.transfer);
      send c.worker, eCommitted;
    }
  }

  fun Apply(account: int, transfer: int, delta: int) {
    balances[account] = balances[account] + delta;
    announce eApplied, (account = account, transfer = transfer, balance = balances[account]);
  }

  fun Release(account: int, transfer: int) {
    var next: tWaiter;
    announce eLockReleased, (account = account, transfer = transfer);
    holder -= (account);
    if (sizeof(waiters[account]) > 0) {
      next = waiters[account][0];
      waiters[account] -= (0);
      Grant(next);
    }
  }

  fun Grant(w: tWaiter) {
    holder[w.account] = w.transfer;
    announce eLockGranted, (account = w.account, transfer = w.transfer);
    send w.worker, eLocked, (account = w.account,);
  }
}
