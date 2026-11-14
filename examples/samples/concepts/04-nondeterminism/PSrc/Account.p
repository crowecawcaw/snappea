// The design: an account that pays out a withdrawal only if the balance covers it.
// Its database sometimes times out; the account then answers "unavailable" and changes
// nothing. The model does not decide when that happens, so it is a choice ($).

event Withdraw: (client: machine, amount: int);
event WithdrawResponse: (ok: bool, balance: int);
event Unavailable;

machine Account {
  var balance: int;

  start state Serving {
    entry (initial: int) {
      balance = initial;
    }
    on Withdraw do (w: (client: machine, amount: int)) {
      // Environment choice: the database call times out, or it does not. The checker
      // tries both, every time this handler runs.
      if ($) {
        send w.client, Unavailable;
      } else if (w.amount <= balance) {
        balance = balance - w.amount;
        send w.client, WithdrawResponse, (ok = true, balance = balance);
      } else {
        send w.client, WithdrawResponse, (ok = false, balance = balance);
      }
    }
  }
}
