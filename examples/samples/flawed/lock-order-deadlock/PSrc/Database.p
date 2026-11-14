// A database with row locks, as in SELECT ... FOR UPDATE. A transfer runs as one
// transaction: lock the rows it needs, one statement at a time; then commit, which
// applies both balance changes and releases both locks. A lock request on a row
// another transaction holds waits until that transaction commits.
//
// Real primitives (each atomic): lock(row), commit(transaction). Between the two lock
// statements of one transaction, other transactions run.

event Lock: (worker: machine, transfer: int, account: int);
event Locked: (account: int);
event Commit: (worker: machine, transfer: Transfer);
event Committed;

type Waiter = (worker: machine, transfer: int, account: int);
type Transfer = (transfer: int, src: int, dst: int, amount: int);

machine Database {
  var balances: map[int, int];
  // account -> transaction holding its row lock
  var holder: map[int, int];
  // account -> lock requests waiting for it, in arrival order
  var waiters: map[int, seq[Waiter]];

  start state Serving {
    entry (accounts: map[int, int]) {
      var account: int;
      balances = accounts;
      foreach (account in keys(balances)) {
        waiters[account] = default(seq[Waiter]);
      }
    }
    on Lock do (w: Waiter) {
      if (w.account in holder) {
        waiters[w.account] += (sizeof(waiters[w.account]), w);
        return;
      }
      Grant(w);
    }
    on Commit do (c: (worker: machine, transfer: Transfer)) {
      Apply(c.transfer.src, c.transfer.transfer, -c.transfer.amount);
      Apply(c.transfer.dst, c.transfer.transfer, c.transfer.amount);
      Release(c.transfer.src, c.transfer.transfer);
      Release(c.transfer.dst, c.transfer.transfer);
      send c.worker, Committed;
    }
  }

  fun Apply(account: int, transfer: int, delta: int) {
    balances[account] = balances[account] + delta;
    announce Applied, (account = account, transfer = transfer, balance = balances[account]);
  }

  fun Release(account: int, transfer: int) {
    var next: Waiter;
    announce LockReleased, (account = account, transfer = transfer);
    holder -= (account);
    if (sizeof(waiters[account]) > 0) {
      next = waiters[account][0];
      waiters[account] -= (0);
      Grant(next);
    }
  }

  fun Grant(w: Waiter) {
    holder[w.account] = w.transfer;
    announce LockGranted, (account = w.account, transfer = w.transfer);
    send w.worker, Locked, (account = w.account,);
  }
}
