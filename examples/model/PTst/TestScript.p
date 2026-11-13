// The atomic design keeps the spec.
test tcAtomicReserve [main=AtomicReserve]:
  assert InventoryCorrect, OrdersAnswered in { Inventory, OrderClient, AtomicReserve };

// The check-then-reserve design oversells: two orders both see 3 units available.
// This test case is expected to find that bug.
test tcCheckThenReserve [main=CheckThenReserve]:
  assert InventoryCorrect, OrdersAnswered in { Inventory, OrderClient, CheckThenReserve };

// Every order gets an answer: hot while a request has no reply. Specs observe sent events
// as well as announced ones, so the model needs no extra announcements.
spec OrdersAnswered observes eReserveReq, eAvailableReq, eCommitReq, eReserveResp, eAvailableResp {
  var pending: int;

  start state Answered {
    on eReserveReq do Asked;
    // snappea-lint: allow coverage -- check-then-reserve requests, exercised by tcCheckThenReserve
    on eAvailableReq, eCommitReq do Asked;
  }

  hot state Waiting {
    on eReserveReq do Asked;
    on eReserveResp do Replied;
    // snappea-lint: allow coverage -- check-then-reserve requests, exercised by tcCheckThenReserve
    on eAvailableReq, eCommitReq do Asked;
    // snappea-lint: allow coverage -- check-then-reserve reply, exercised by tcCheckThenReserve
    on eAvailableResp do Replied;
  }

  fun Asked() {
    pending = pending + 1;
    goto Waiting;
  }

  fun Replied() {
    pending = pending - 1;
    if (pending == 0) {
      goto Answered;
    }
  }
}
