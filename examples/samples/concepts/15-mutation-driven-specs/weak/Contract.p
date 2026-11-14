// The FIRST version of the contract, kept to show what mutation testing reports for it.
// It is not part of MutationDriven.pproj. Run it with:
//   snappea check PSrc/Account.p PSpec/Events.p weak/Contract.p PTst/Tests.p --warn lint
spec Contract observes Withdrawn {
  start state Watching {
    on Withdrawn do (w: (id: int, ok: bool, balance: int)) {
      assert w.balance >= 0, format("balance went negative: {0}", w.balance);
    }
  }
}
