// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

test tcSaneUserUsingCoffeeMachine [main=TestWithSaneUser]:
assert EspressoMachineModesOfOperation in (union {
  TestWithSaneUser
}
, EspressoMachine, Users);

test tcCrazyUserUsingCoffeeMachine [main=TestWithCrazyUser]:
assert EspressoMachineModesOfOperation in (union {
  TestWithCrazyUser
}
, EspressoMachine, Users);

// BASIC PARAMETER TESTS

// test with a parameterized number of operations for the crazy user
test param (nOps in [4, 5, 6]) tcCrazyUserParamOps [main=TestWithConfig]:
assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, {
  TestWithConfig
}
);

// MULTI-PARAMETER TESTS

// The spec follows one coffee machine's modes, so each test drives a single machine.
// Two machines would interleave their announcements and look like one machine
// skipping modes.

// Test resource constraints with different levels
test param (waterLevel in [0, 25, 50, 100], beanLevel in [0, 25, 50]) tcResourceConstraints [main=TestWithResourceConstraints]:
assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, {
  TestWithResourceConstraints
}
);

// BOOLEAN PARAMETER TESTS

// Test boolean configurations
test param (enableSteamer in [true, false], cleaningMode in [true, false]) tcBooleanConfigs [main=TestWithMixedConfiguration]:
assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, {
  TestWithMixedConfiguration, SteamerTestUser
}
);
