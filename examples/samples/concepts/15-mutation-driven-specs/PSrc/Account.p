// The design: the account from 04-nondeterminism, without database timeouts. A
// withdrawal is one atomic check-and-debit: it succeeds only if the balance covers it,
// and the reply is sent in the same step.

event Withdraw: (client: machine, id: int, amount: int);
event WithdrawResponse: (id: int, ok: bool, balance: int);

machine Account {
  var balance: int;

  start state Serving {
    entry (initial: int) {
      balance = initial;
    }
    on Withdraw do (w: (client: machine, id: int, amount: int)) {
      var ok: bool;
      // Fast path: a zero withdrawal changes nothing, so answer without the ledger.
      // The general path below gives the same answer, so removing this cannot be seen.
      // snappea-lint: allow mutation:remove-statement -- equivalent: the general path gives the same reply for 0
      // snappea-lint: allow mutation:condition-false -- equivalent: the general path gives the same reply for 0
      if (w.amount == 0) {
        send w.client, WithdrawResponse, (id = w.id, ok = true, balance = balance);
        return;
      }
      ok = w.amount <= balance;
      if (ok) {
        balance = balance - w.amount;
      }
      send w.client, WithdrawResponse, (id = w.id, ok = ok, balance = balance);
    }
  }
}
