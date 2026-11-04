// Shared environment model: timers and crash injection make nondeterministic choices so
// the checker can explore different delivery and failure orders.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.
// $ is a model choice: the checker explores both outcomes across schedules.

/*
The failure injector machine randomly selects a replica machine and enqueues the special event "halt".
*/
event DelayNodeFailure;
// event: event sent by the failure injector to shutdown a node
event ShutDown: machine;

machine FailureInjector {
  var nFailures: int;
  var nodes: set[machine];

  start state Init {
    entry (config: (nodes: set[machine], nFailures: int)) {
      nFailures = config.nFailures;
      nodes = config.nodes;
      assert nFailures < sizeof(nodes);
      goto FailOneNode;
    }
  }

  state FailOneNode {
    entry {
      var fail: machine;

      if (nFailures == 0) {

        raise halt; // done with all failures

      }
      else {
        if($) {
          fail = choose(nodes);
          send fail, ShutDown, fail;
          nodes -= (fail);
          nFailures = nFailures - 1;
        }

        else {
          send this, DelayNodeFailure;
        }
      }
    }

    on DelayNodeFailure goto FailOneNode;
  }
}

// function to create the failure injection
fun CreateFailureInjector(config: (nodes: set[machine], nFailures: int)) {
  new FailureInjector(config);
}

// failure injector module
module FailureInjector = {
  FailureInjector
}
;
