// Failure detection: the detector sends heartbeats and suspects silent nodes. Delayed
// replies can look like crashes; the rule checks reported failures against actual crashes.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

test tcTest_FailureDetector [main=TestMultipleClients]:
assert ReliableFailureDetector in
union {
  TestMultipleClients
}
, FailureDetector, FailureInjector;

// Test case that ensures clients don't outnumber nodes for better monitoring distribution
test param (numNodes in [3, 4, 5], numClients in [2, 3, 4]) assume (numClients <= numNodes) tcTest_BalancedLoad [main=TestWithConfig]:
assert ReliableFailureDetector in
union {
  TestWithConfig
}
, FailureDetector, FailureInjector;
