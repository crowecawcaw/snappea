// The design: an account that pays out a withdrawal only if the balance covers it.
// Its database sometimes times out; the account then answers "unavailable" and changes
// nothing. The model does not decide when that happens, so it is a choice ($).

event Withdraw: (client: machine, id: int, amount: int);
event WithdrawResponse: (id: int, ok: bool, balance: int);
event Unavailable: (id: int);

machine Account {
  var balance: int;

  start state Serving {
    entry (initial: int) {
      balance = initial;
    }
    // One atomic check-and-debit, answered in the same step.
    on Withdraw do (w: (client: machine, id: int, amount: int)) {
      // Environment choice: the database call times out, or it does not. The checker
      // tries both, every time this handler runs.
      if ($) {
        send w.client, Unavailable, (id = w.id,);
      } else if (w.amount <= balance) {
        balance = balance - w.amount;
        send w.client, WithdrawResponse, (id = w.id, ok = true, balance = balance);
      } else {
        send w.client, WithdrawResponse, (id = w.id, ok = false, balance = balance);
      }
    }
  }
}
