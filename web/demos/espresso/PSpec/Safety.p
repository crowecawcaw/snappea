// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

/* Events used to inform monitor about the internal state of the CoffeeMaker */
event InWarmUpState;
event InReadyState;
event InBeansGrindingState;
event InCoffeeBrewingState;
event ErrorHappened;
event ResetPerformed;

// We would like to ensure that the coffee maker moves through the expected modes of
// operation. We want to make sure that the coffee maker always transitions through the
// following sequence of states: Steady operation: WarmUp -> Ready -> GrindBeans ->
// MakeCoffee -> Ready With Error: If an error occurs in any of the states above then the
// Coffee machine stays in the error state until it is reset and after which it returns to
// the Warmup state.
spec EspressoMachineModesOfOperation
observes InWarmUpState, InReadyState, InBeansGrindingState, InCoffeeBrewingState, ErrorHappened, ResetPerformed
{
  start state StartUp {
    on InWarmUpState goto WarmUp;
  }

  state WarmUp {
    on ErrorHappened goto Error;
    on InReadyState goto Ready;
  }

  state Ready {
    ignore InReadyState;
    on InBeansGrindingState goto BeanGrinding;
    on ErrorHappened goto Error;
  }

  state BeanGrinding {
    on InCoffeeBrewingState goto MakingCoffee;
    on ErrorHappened goto Error;
  }

  state MakingCoffee {
    on InReadyState goto Ready;
    on ErrorHappened goto Error;
  }

  state Error {
    on ResetPerformed goto StartUp;
    ignore ErrorHappened;
  }
}
