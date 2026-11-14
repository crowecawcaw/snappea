// The downstream consumer. The broker delivers at least once, and relays publish some
// orders twice, so the consumer deduplicates by message id: it applies an order and
// records its id in one local transaction, and drops a message whose id it has seen.

event Deliver: int;

machine Consumer {
  var processed: set[int];

  start state Consuming {
    on Deliver do (id: int) {
      if (id in processed) { return; }
      processed += (id);
      announce Applied, id;
    }
  }
}
