enum Status { OK, FAILED }
type Item = (name: string, qty: int);
event eOpen: (id: int, owner: string, items: seq[Item], tags: set[string]);
event eUpdate: (id: int, prices: map[string, float], note: any);
event eClose: (id: int, status: Status, ok: bool);

spec OrdersComplete observes eOpen, eUpdate, eClose {
  var open: map[int, int];
  start state Watching {
    on eOpen do (p: (id: int, owner: string, items: seq[Item], tags: set[string])) {
      assert !(p.id in open), format("order {0} opened twice", p.id);
      open[p.id] = sizeof(p.items);
    }
    on eUpdate do (p: (id: int, prices: map[string, float], note: any)) {
      assert p.id in open, format("order {0} updated before open", p.id);
    }
    on eClose do (p: (id: int, status: Status, ok: bool)) {
      assert p.id in open, format("order {0} closed before open", p.id);
      assert (p.status == OK) == p.ok, "status and ok disagree";
      open -= (p.id);
    }
  }
}
