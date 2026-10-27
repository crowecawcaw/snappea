// snappea: driver
machine Main {
  var queue: machine;
  start state Run {
    entry {
      queue = new PriorityQueue();
      // Equal-priority admissions preserve insertion order.
      Call(0, 0, 0); Call(0, 1, 0); Call(3, 0, 0); Call(3, 0, 0);
      // Priority changes preserve the earlier admission position.
      Call(0, 0, 1); Call(0, 1, 0); Call(1, 0, 0);
      Call(3, 0, 0); Call(3, 0, 0);
      // Re-admission has a fresh position, unlike priority updates.
      Call(0, 0, 0); Call(0, 1, 0); Call(2, 0, 0); Call(0, 0, 0);
      Call(3, 0, 0); Call(3, 0, 0);
      // Negative priorities compare as signed integers.
      Call(0, 0, -2); Call(0, 1, 1); Call(3, 0, 0); Call(3, 0, 0);
      // Rejected duplicate arguments must not mutate the active item.
      Call(0, 0, -1); Call(0, 1, 1); Call(0, 0, 2);
      Call(3, 0, 0); Call(3, 0, 0); Call(3, 0, 0);
      Call(1, 0, 2); Call(2, 1, 0);
      send queue, eFinish;
    }
  }
  fun Call(op: int, id: int, priority: int) {
    send queue, eCommand, (op=op, id=id, priority=priority, reply=this);
    receive { case eAck: { } }
  }
}
