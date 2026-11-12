// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

/* Events used by the user to interact with the control panel of the Coffee Machine */
// event: make espresso button pressed
event EspressoButtonPressed;
// event: steamer button turned off
event SteamerButtonOff;
// event: steamer button turned on
event SteamerButtonOn;
// event: door opened to empty grounds
event OpenGroundsDoor;
// event: door closed after emptying grounds
event CloseGroundsDoor;
// event: reset coffee maker button pressed
event ResetCoffeeMaker;
//event: error message from panel to the user
event CoffeeMakerError: CoffeeMakerState;
//event: coffee machine is ready
event CoffeeMakerReady;
// event: coffee machine user
event CoffeeMachineUser: machine;

// enum to represent the state of the coffee maker
enum CoffeeMakerState {
  NotWarmedUp,
  Ready,
  NoBeansError,
  NoWaterError,
  HeaterError
}

/*
CoffeeMakerControlPanel acts as the interface between the CoffeeMaker and User
It converts the inputs from the user to appropriate inputs to the CoffeeMaker and sends responses to
the user.
*/
machine CoffeeMakerControlPanel {
  var coffeeMaker: EspressoCoffeeMaker;
  var coffeeMakerState: CoffeeMakerState;
  var currentUser: machine;

  start state Init {
    entry {
      coffeeMakerState = NotWarmedUp;
      coffeeMaker = new EspressoCoffeeMaker(this);
      WaitForUser();
      goto WarmUpCoffeeMaker;
    }
  }

  // block until a user shows up
  fun WaitForUser() {
    receive {
      case CoffeeMachineUser: (user: machine) {
        currentUser = user;
      }
    }
  }

  state WarmUpCoffeeMaker {
    entry {
      // inform the specification about current state of the coffee maker
      announce InWarmUpState;

      BeginHeatingCoffeeMaker();
    }

    on WarmUpCompleted goto CoffeeMakerReady;

    on WarmerError goto EncounteredError with {
      coffeeMakerState = HeaterError;
      print "The heater is broken! Please repair it and reset the machine!";
    }

    // grounds door is opened or closed will handle it later after the coffee maker has warmed up
    defer OpenGroundsDoor, CloseGroundsDoor;
    // ignore these inputs from users until the maker has warmed up.
    ignore EspressoButtonPressed, SteamerButtonOff, SteamerButtonOn, ResetCoffeeMaker;
    // ignore these errors and responses as they could be from previous state
    ignore BeansDepleted, WaterDepleted, GrindBeansCompleted;
  }

  state CoffeeMakerReady {
    entry {
      // inform the specification about current state of the coffee maker
      announce InReadyState;

      coffeeMakerState = Ready;
      send currentUser, CoffeeMakerReady;
    }

    on OpenGroundsDoor goto CoffeeMakerDoorOpened;
    on EspressoButtonPressed goto CoffeeMakerRunGrind;
    on SteamerButtonOn goto CoffeeMakerRunSteam;

    // ignore these out of order commands, these must have happened because of an error
    // from user or sensor
    ignore SteamerButtonOff, CloseGroundsDoor;

    // ignore commands and errors as they are from previous state
    ignore WarmUpCompleted, ResetCoffeeMaker, BeansDepleted, WaterDepleted;
  }

  state CoffeeMakerRunGrind {
    entry {
      // inform the specification about current state of the coffee maker
      announce InBeansGrindingState;

      GrindBeans();
    }

    on BeansDepleted goto EncounteredError with {
      coffeeMakerState = NoBeansError;
      print "No beans to grind! Please refill beans and reset the machine!";
    }

    on WaterDepleted goto EncounteredError with {
      coffeeMakerState = NoWaterError;
      print "No Water! Please refill water and reset the machine!";
    }

    on GrindBeansCompleted goto CoffeeMakerRunEspresso;

    defer OpenGroundsDoor, CloseGroundsDoor, EspressoButtonPressed;

    // Can't make steam while we are making espresso
    ignore SteamerButtonOn, SteamerButtonOff;

    // ignore commands that are old or cannot be handled right now
    ignore WarmUpCompleted, ResetCoffeeMaker;
  }

  state CoffeeMakerRunEspresso {
    entry {
      // inform the specification about current state of the coffee maker
      announce InCoffeeBrewingState;

      StartEspresso();
    }

    on EspressoCompleted goto CoffeeMakerReady with {
      send currentUser, EspressoCompleted;
    }

    on WaterDepleted goto EncounteredError with {
      coffeeMakerState = NoWaterError;
      print "No Water! Please refill water and reset the machine!";
    }

    // the user commands will be handled next after finishing this espresso
    defer OpenGroundsDoor, CloseGroundsDoor, EspressoButtonPressed;

    // Can't make steam while we are making espresso
    ignore SteamerButtonOn, SteamerButtonOff;

    // ignore old commands and cannot reset when making coffee
    ignore WarmUpCompleted, ResetCoffeeMaker;
  }

  state CoffeeMakerRunSteam {
    entry {
      StartSteamer();
    }

    on SteamerButtonOff  goto CoffeeMakerReady with {
      StopSteamer();
    }

    on WaterDepleted goto EncounteredError with {
      coffeeMakerState = NoWaterError;
      print "No Water! Please refill water and reset the machine!";
    }

    // user might have cleaned grounds while steaming
    defer OpenGroundsDoor, CloseGroundsDoor;

    // can't make espresso while we are making steam
    ignore EspressoButtonPressed, SteamerButtonOn;
  }

  state CoffeeMakerDoorOpened {
    on CloseGroundsDoor do {
      assert coffeeMakerState != NotWarmedUp;
      assert coffeeMakerState == Ready;
      goto CoffeeMakerReady;
    }

    // grounds door is open cannot handle these requests just ignore them
    ignore EspressoButtonPressed, SteamerButtonOn, SteamerButtonOff;

    // ignore errors from an earlier steam request; the door was opened after it finished
    ignore BeansDepleted, WaterDepleted;
  }

  state EncounteredError {
    entry {
      // inform the specification about current state of the coffee maker
      announce ErrorHappened;

      // send the error message to the client
      send currentUser, CoffeeMakerError, coffeeMakerState;
    }

    on ResetCoffeeMaker goto WarmUpCoffeeMaker with {
      // inform the specification about current state of the coffee maker
      announce ResetPerformed;
    }

    // error, ignore these requests until reset.
    ignore EspressoButtonPressed, SteamerButtonOn, SteamerButtonOff,
    OpenGroundsDoor, CloseGroundsDoor, WarmUpCompleted, EspressoCompleted, GrindBeansCompleted;

    // ignore other simultaneous errors
    ignore BeansDepleted, WaterDepleted;
  }

  fun BeginHeatingCoffeeMaker() {
    // send an event to maker to start warming
    send coffeeMaker, WarmUpRequest;
  }

  fun StartSteamer() {
    // send an event to maker to start steaming
    send coffeeMaker, StartSteamerRequest;
  }

  fun StopSteamer() {
    // send an event to maker to stop steaming
    // snappea-lint: allow unchecked-event -- the model has no steamer state to change; the real maker stops the steamer here
    send coffeeMaker, StopSteamerRequest;
  }

  fun GrindBeans() {
    // send an event to maker to grind beans
    send coffeeMaker, GrindBeansRequest;
  }

  fun StartEspresso() {
    // send an event to maker to start espresso
    send coffeeMaker, StartEspressoRequest;
  }
}
