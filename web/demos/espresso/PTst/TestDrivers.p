// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

machine TestWithSaneUser {
  start state Init {
    entry {
      // create a sane user
      new SaneUser(new CoffeeMakerControlPanel());
    }
  }
}

machine TestWithCrazyUser {
  start state Init {
    entry {
      // create a crazy user
      new CrazyUser((coffeeMaker = new CoffeeMakerControlPanel(), nOps = 5));
    }
  }
}

// Parameters for comprehensive testing scenarios
param nOps : int;
param waterLevel : int;
param beanLevel : int;
param enableSteamer : bool;
param cleaningMode : bool;

machine TestWithConfig {
  start state Init {
    entry {
      // create a crazy user
      new CrazyUser((coffeeMaker = new CoffeeMakerControlPanel(), nOps = nOps));
    }
  }
}

// Test driver with resource constraints (water and bean levels)
machine TestWithResourceConstraints {
  start state Init {
    entry {
      var coffeeMaker: CoffeeMakerControlPanel;
      coffeeMaker = new CoffeeMakerControlPanel();

      if (waterLevel > 0 && beanLevel > 0) {
        new SaneUser(coffeeMaker);
      }
      else {
        new CrazyUser((coffeeMaker = coffeeMaker, nOps = 2));
      }
    }
  }
}

// Test driver with mixed configurations
machine TestWithMixedConfiguration {
  start state Init {
    entry {
      var coffeeMaker: CoffeeMakerControlPanel;
      coffeeMaker = new CoffeeMakerControlPanel();

      if (cleaningMode) {
        new CrazyUser((coffeeMaker = coffeeMaker, nOps = 3));
      }
      else if (enableSteamer) {
        new SteamerTestUser(coffeeMaker);
      }
      else {
        new SaneUser(coffeeMaker);
      }
    }
  }
}
