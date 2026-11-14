// A server with a fixed number of session slots. When a new connection arrives and every
// slot is taken, it evicts the current holder and refuses the newcomer, which may retry.
// A slot is freed only when its connection says goodbye (Fin).

enum Reason { ByClient, Evicted, CapacityRefused }

event Syn: machine;
event SynAck: (ok: bool, session: int);
event Fin: int;
event Reset: Reason;

machine Server {
  // slot -> connection holding it, or null when free
  var slots: seq[machine];

  start state Serving {
    entry (capacity: int) {
      while (sizeof(slots) < capacity) {
        slots += (sizeof(slots), null);
      }
    }
    on Syn do (conn: machine) {
      var slot: int;
      slot = FreeSlot();
      if (slot < 0) {
        announce Rejected, conn;
        // Make room for the next attempt: evict the oldest holder.
        send slots[0], Reset, Evicted;
        send conn, SynAck, (ok = false, session = -1);
        return;
      }
      slots[slot] = conn;
      announce Accepted, (conn = conn, session = slot);
      send conn, SynAck, (ok = true, session = slot);
    }
    on Fin do (session: int) {
      announce Freed, session;
      slots[session] = null;
    }
  }

  // A function with a return value and an early return from a loop.
  fun FreeSlot(): int {
    var i: int;
    while (i < sizeof(slots)) {
      if (slots[i] == null) { return i; }
      i = i + 1;
    }
    return -1;
  }
}
