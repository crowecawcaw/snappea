// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

/*
A SaneUser who knows how to use the CoffeeMaker
*/
machine SaneUser {
  var coffeeMakerPanel: CoffeeMakerControlPanel;
  var cups: int;
  start state Init {
    entry (coffeeMaker: CoffeeMakerControlPanel) {
      coffeeMakerPanel = coffeeMaker;
      // inform control panel that I am the user
      send coffeeMakerPanel, CoffeeMachineUser, this;
      // want to make 2 cups of espresso
      cups = 2;

      goto LetsMakeCoffee;
    }
  }

  state LetsMakeCoffee {
    entry {
      while (cups > 0)
      {
        // lets wait for coffee maker to be ready
        WaitForCoffeeMakerToBeReady();

        // press Espresso button
        PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressEspressoButton);

        // check the status of the machine
        receive {
          case EspressoCompleted: {
            // lets make the next coffee
            cups = cups - 1;
          }

          case CoffeeMakerError: (status: CoffeeMakerState){
            // lets fill the beans or water and reset the machine
            // and go back to making espresso
            PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressResetButton);
          }
        }
      }

      // I am a good user so I will clear the coffee grounds before leaving.
      PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_ClearGrounds);

      // am done, let me exit
      raise halt;
    }
  }
}

enum CoffeeMakerOperations {
  CM_PressEspressoButton,
  CM_PressSteamerButton,
  CM_PressResetButton,
  CM_ClearGrounds
}

/*
A crazy user who gets excited by looking at a coffee machine and starts stress testing the machine
by pressing all sorts of random button and opening/closing doors
*/
// TODO: We do not support global constants currently, they can be encoded using global functions.

machine CrazyUser {
  var coffeeMakerPanel: CoffeeMakerControlPanel;
  var numOperations: int;
  start state StartPressingButtons {
    entry (config: (coffeeMaker: CoffeeMakerControlPanel, nOps: int)) {
      var pickedOps: CoffeeMakerOperations;

      numOperations = config.nOps;
      coffeeMakerPanel = config.coffeeMaker;

      // inform control panel that I am the user
      send coffeeMakerPanel, CoffeeMachineUser, this;

      while(numOperations > 0)
      {
        pickedOps = PickRandomOperationToPerform();
        PerformOperationOnCoffeeMaker(coffeeMakerPanel, pickedOps);
        numOperations = numOperations - 1;
      }
    }

    // I will ignore all the responses from the coffee maker
    ignore CoffeeMakerError, EspressoCompleted, CoffeeMakerReady;
  }

  // Pick a random enum value (hacky work around)
  // Currently, the choose operation does not support choose over enum value
  fun PickRandomOperationToPerform() : CoffeeMakerOperations {
    var op_i: int;
    op_i =  choose(3);
    if(op_i == 0)
    return CM_PressEspressoButton;
    else if(op_i == 1)
    return CM_PressSteamerButton;
    else if(op_i == 2)
    return CM_PressResetButton;
    else
    return CM_ClearGrounds;
  }
}

// Specialized user for testing steamer functionality
machine SteamerTestUser {
  var coffeeMakerPanel: CoffeeMakerControlPanel;

  start state Init {
    entry (coffeeMaker: CoffeeMakerControlPanel) {
      coffeeMakerPanel = coffeeMaker;
      send coffeeMakerPanel, CoffeeMachineUser, this;
      goto TestSteamer;
    }
  }

  state TestSteamer {
    entry {
      // Test steamer operations specifically
      PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressSteamerButton);

      receive {
        case CoffeeMakerReady: {
          // Continue with espresso after steamer test
          PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressEspressoButton);
        }

        case CoffeeMakerError: (status: CoffeeMakerState) {
          // Reset and try again
          PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressResetButton);
        }
      }
    }

    ignore EspressoCompleted;
  }
}

/* Function to perform an operation on the CoffeeMaker */
fun PerformOperationOnCoffeeMaker(coffeeMakerCP: CoffeeMakerControlPanel, CM_Ops: CoffeeMakerOperations)
{
  if(CM_Ops == CM_PressEspressoButton) {
    send coffeeMakerCP, EspressoButtonPressed;
  }

  else if(CM_Ops == CM_PressSteamerButton) {
    send coffeeMakerCP, SteamerButtonOn;
    // wait for some time and then release the button
    send coffeeMakerCP, SteamerButtonOff;
  }

  else if(CM_Ops == CM_ClearGrounds)
  {
    send coffeeMakerCP, OpenGroundsDoor;
    // empty ground and close the door
    send coffeeMakerCP, CloseGroundsDoor;
  }

  else if(CM_Ops == CM_PressResetButton)
  {
    send coffeeMakerCP, ResetCoffeeMaker;
  }
}

fun WaitForCoffeeMakerToBeReady() {
  receive {
    case CoffeeMakerReady: {
    }

    case CoffeeMakerError: (status: CoffeeMakerState){
      raise halt;
    }
  }
}

module Users = {
  SaneUser, CrazyUser
}
;
