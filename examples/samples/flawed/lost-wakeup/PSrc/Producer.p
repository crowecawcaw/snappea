// The producer puts its items one after another. Shared by both designs: the flaw is in
// how consumers wait, not in how items are put.

event ePut: int;

machine Producer {
  start state Producing {
    entry (p: (queue: machine, items: seq[int])) {
      var item: int;
      foreach (item in p.items) {
        send p.queue, ePut, item;
      }
    }
  }
}
