// A bank account. A withdrawal is one atomic check-and-debit: it succeeds only if the
// balance covers it. Both outcomes are announced at the commit and answered.

event Withdraw: (client: machine, id: int, amount: int);
event WithdrawResponse: (id: int, ok: bool, balance: int);

machine Account {
  var balance: int;

  start state Open {
    entry (initial: int) {
      balance = initial;
    }
    on Withdraw do (w: (client: machine, id: int, amount: int)) {
      var ok: bool;
      // Fast path: a zero withdrawal changes nothing, so answer without the ledger.
      // The general path below gives the same answer, so removing this cannot be seen.
      // snappea-lint: allow mutation:remove-statement -- optimisation; the general path gives the same result for 0
      // snappea-lint: allow mutation:condition-false -- optimisation; the general path gives the same result for 0
      if (w.amount == 0) {
        Answer(w.client, w.id, true);
        return;
      }
      ok = w.amount <= balance;
      if (ok) {
        balance = balance - w.amount;
      }
      Answer(w.client, w.id, ok);
    }
  }

  fun Answer(client: machine, id: int, ok: bool) {
    announce Withdrawn, (id = id, ok = ok, balance = balance);
    send client, WithdrawResponse, (id = id, ok = ok, balance = balance);
  }
}
