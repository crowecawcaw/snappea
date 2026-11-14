// The FIRST version of the contract, kept to show what mutation testing reports for it.
// It is not part of MutationDriven.pproj. Run it with:
//   snappea check PSrc/Account.p PSpec/Events.p weak/Withdrawals.p PTst/Tests.p --warn lint
spec WithdrawalsMatchBalance observes WithdrawResponse {
  start state Watching {
    on WithdrawResponse do (r: (id: int, ok: bool, balance: int)) {
      assert r.balance >= 0, format("withdrawal {0} left balance {1}", r.id, r.balance);
    }
  }
}
