// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.
// $ is a model choice: the checker explores both outcomes across schedules.

/* Requests or operations from the controller to coffee maker */

// event: warmup request when the coffee maker starts or resets
event WarmUpRequest;
// event: grind beans request before making coffee
event GrindBeansRequest;
// event: start brewing coffee
event StartEspressoRequest;
// event start steamer
event StartSteamerRequest;
// event: stop steamer
event StopSteamerRequest;

/* Responses from the coffee maker to the controller */
// event: completed grinding beans
event GrindBeansCompleted;
// event: completed brewing and pouring coffee
event EspressoCompleted;
// event: warmed up the machine and read to make coffee
event WarmUpCompleted;

/* Error messages from the coffee maker to control panel or controller*/
// event: no water for coffee, refill water!
event WaterDepleted;
// event: no beans for coffee, refill beans!
event BeansDepleted;
// event: the heater to warm the machine is broken!
event WarmerError;

// EspressoCoffeeMaker receives requests from the control panel of the coffee machine and
// based on its state e.g., whether heater is working, or it has beans and water, the maker
// responds back to the controller if the operation succeeded or errored.
machine EspressoCoffeeMaker {
  // control panel of the coffee machine that sends inputs to the coffee maker
  var controller: CoffeeMakerControlPanel;

  start state WaitForRequests {
    entry (_controller: CoffeeMakerControlPanel) {
      controller = _controller;
    }

    on WarmUpRequest do {
      send controller, WarmUpCompleted;
    }

    on GrindBeansRequest do {
      if (!HasBeans()) {
        send controller, BeansDepleted;
      }
      else {
        send controller, GrindBeansCompleted;
      }
    }

    on StartEspressoRequest do {
      if (!HasWater()) {
        send controller, WaterDepleted;
      }
      else {
        send controller, EspressoCompleted;
      }
    }

    on StartSteamerRequest do {
      if (!HasWater()) {
        send controller, WaterDepleted;
      }
    }

    on StopSteamerRequest do {
      /* do nothing, steamer stopped */
    }
  }

  // nondeterministic functions to trigger different behaviors
  fun HasBeans() : bool {
    return $;
  }

  fun HasWater() : bool {
    return $;
  }
}
