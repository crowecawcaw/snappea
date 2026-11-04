// Bank withdrawals: clients ask the server to debit accounts. The database owns the
// balances. The abstract-server test intentionally violates the minimum balance rule.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

/* This file contains three different model checking scenarios */

// assert the properties for the single client and single server scenario
test tcSingleClient [main=TestWithSingleClient]:
assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
(union Client, Bank, {
  TestWithSingleClient
}
);

// assert the properties for the two clients and single server scenario
test tcMultipleClients [main=TestWithMultipleClients]:
assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
(union Client, Bank, {
  TestWithMultipleClients
}
);

// assert the properties for the single client and single server scenario but with abstract server
test tcAbstractServer [main=TestWithSingleClient]:
assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
(union Client, AbstractBank, {
  TestWithSingleClient
}
);

// test with a parameterized number of clients
test param (nClients in [2, 3, 4]) tcParameterizedMultipleClients [main=TestWithConfig]:
assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
(union Client, Bank, {
  TestWithConfig
}
);
