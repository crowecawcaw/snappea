// snappea: driver
machine Main {
  var queue: machine;
  start state Run {
    entry {
      queue = new PriorityQueue();
      Call(0, 0, -1);
      Call(0, 1, 1);
      Call(0, 0, -1);
      Call(1, 0, -1);
      Call(2, 1, 0);
      Call(0, 1, 1);
      Call(3, 0, 0);
      Call(3, 0, 0);
      Call(3, 0, 0);
      Call(2, 0, 0);
      Call(1, 1, 0);
      send queue, eFinish;
    }
  }
  fun Call(op: int, id: int, priority: int) {
    send queue, eCommand, (op=op, id=id, priority=priority, reply=this);
    receive { case eAck: { } }
  }
}
