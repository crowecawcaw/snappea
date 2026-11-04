// Coffee maker: users press controls while sensors report heating, grinding, and brewing.
// The safety rule checks that brewing starts only in a valid state.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

module EspressoMachine = {
  CoffeeMakerControlPanel, EspressoCoffeeMaker
}
;
