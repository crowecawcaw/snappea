test tcAtomicConditionalReserve [main=AtomicCase]:
  assert Capacity in { Store, Client, AtomicCase };

test tcTwoSequentialSuccesses [main=TwoReservationsCase]:
  assert Capacity in { Store, Client, TwoReservationsCase };
