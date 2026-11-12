// The design: an account that pays out a withdrawal only if the balance covers it.
// Its database sometimes times out; the account then answers "unavailable" and changes
// nothing. The model does not decide when that happens, so it is a choice ($).

event eWithdraw: (client: machine, amount: int);
event eWithdrawResp: (ok: bool, balance: int);
event eUnavailable;

machine Account {
  var balance: int;

  start state Serving {
    entry (initial: int) {
      balance = initial;
    }
    on eWithdraw do (w: (client: machine, amount: int)) {
      // Environment choice: the database call times out, or it does not. The checker
      // tries both, every time this handler runs.
      if ($) {
        send w.client, eUnavailable;
      } else if (w.amount <= balance) {
        balance = balance - w.amount;
        send w.client, eWithdrawResp, (ok = true, balance = balance);
      } else {
        send w.client, eWithdrawResp, (ok = false, balance = balance);
      }
    }
  }
}
