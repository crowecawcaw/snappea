// Contract monitor shared by the model and the Python observation tests.
// Expected results come from the request history, not from the implementation.
event eSpent: (id: string, amount: int, ok: bool);
event eBalance: (balance: int);

spec WalletContract observes eSpent, eBalance {
  var balance: int;
  var results: map[string, bool];
  start state Tracking {
    entry { balance = 10; }
    on eSpent do (p: (id: string, amount: int, ok: bool)) {
      if (p.id in results) {
        assert p.ok == results[p.id], format("retry of {0} changed its result", p.id);
      } else {
        assert p.ok == (p.amount <= balance), format("wrong result for {0}", p.id);
        results[p.id] = p.ok;
        if (p.ok) { balance = balance - p.amount; }
      }
    }
    on eBalance do (p: (balance: int)) {
      assert p.balance == balance, format("balance {0}, expected {1}", p.balance, balance);
    }
  }
}
