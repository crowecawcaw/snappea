"""Saved regressions: real calls, controlled races, and the model's independent spec."""
from concurrent.futures import ThreadPoolExecutor
import importlib.util
from pathlib import Path
from threading import Barrier, Lock

import pytest
from snappea import SpecViolation, check_events  # pip install pysnappea

from queue_store import Queue
from store import Store

HERE = Path(__file__).parent


class SameVersionStore:
    """Arm after initialization; match the next two loads, never a private key name."""
    def __init__(self, inner):
        self.inner, self.lock = inner, Lock()
        self.remaining, self.arrived, self.conflicts = 0, 0, 0
        self.barrier = Barrier(2, timeout=5)

    def arm(self):
        self.remaining = 2

    def load(self):
        result = self.inner.load()
        with self.lock:
            wait = self.remaining > 0
            if wait:
                self.remaining -= 1
                self.arrived += 1
        if wait:
            self.barrier.wait()
        return result

    def compare_swap(self, version, value):
        ok = self.inner.compare_swap(version, value)
        with self.lock:
            self.conflicts += not ok
        return ok


class InterferingStore:
    """Run one real competitor after this load, before this caller can CAS."""
    def __init__(self, inner, competitor):
        self.inner, self.competitor = inner, competitor
        self.fired, self.conflicts = 0, 0

    def load(self):
        result = self.inner.load()
        if self.competitor is not None:
            run, self.competitor = self.competitor, None
            self.fired += 1
            run()
        return result

    def compare_swap(self, version, value):
        ok = self.inner.compare_swap(version, value)
        self.conflicts += not ok
        return ok


def test_two_puts_read_the_same_version():
    store = SameVersionStore(Store())
    a, b = Queue(store, 1), Queue(store, 1)
    store.arm()
    with ThreadPoolExecutor(2) as pool:
        calls = [pool.submit(a.put, 0, 7), pool.submit(b.put, 1, 8)]
        assert sorted(call.result(timeout=10) for call in calls) == [False, True]
    assert store.arrived == 2 and store.conflicts == 1
    assert len(a.snapshot()) == 1
    # Return order across threads is not commit order: use plain assertions here.


def test_losing_cas_recomputes_capacity():
    inner = Store()
    Queue(inner, 1)
    store = InterferingStore(inner, None)
    queue = Queue(store, 1)
    store.competitor = lambda: Queue(inner, 1).put(1, 8)
    assert queue.put(0, 7) is False
    assert store.fired == 1 and store.conflicts == 1
    assert Queue(inner, 1).snapshot() == [8]


def command(identifier, put, item=0):
    return dict(id=identifier, put=put, item=item)


def observe(initial, left, right, queue_type=Queue):
    """Sequential calls have a known commit order. Never infer one from thread returns."""
    store = Store()
    first = queue_type(store, 1)
    for i, item in enumerate(initial):
        assert first.put(-i - 1, item)
    events = [dict(event="ePlan", payload=dict(capacity=1, initial=initial, left=left, right=right))]
    for client in [0, 1, 0, 1]:
        # Recreate the object every time: a local outcome cache cannot pass this test.
        queue = queue_type(store, 1)
        call = [left, right][client]
        if call['put']:
            result = dict(ok=queue.put(call['id'], call['item']), present=False, value=0)
        else:
            value = queue.take(call['id'])
            # P has no Python None. Preserve the distinction between an empty take and 0.
            result = dict(ok=False, present=value is not None, value=0 if value is None else value)
        events.append(dict(event="eObserved", payload=dict(command=call, result=result, items=queue.snapshot())))
        events.append(dict(event="eReturned", payload=dict(client=client, command=call, result=result)))
    events.append(dict(event="eDone"))
    check_events(events, 'QueueContract', spec_files=[str(HERE / 'Contract.p')], strict=True, check_hot=True)


@pytest.mark.parametrize('initial,left,right', [
    ([], command(0, False), command(1, True, 0)),  # empty retry remains None after a put
    ([], command(0, True, 0), command(1, False)),  # accepted retries do not reapply
    ([7], command(0, True, 0), command(1, False)),  # rejected retry remains False after a take
])
def test_real_calls_and_delayed_retries(initial, left, right):
    observe(initial, left, right)


def test_fifo_and_separate_put_take_ids():
    queue = Queue(Store(), 2)
    assert queue.put(0, 7) and queue.put(1, 8)
    assert queue.take(0) == 7  # same ID, different operation namespace
    assert queue.take(1) == 8


def test_spec_rejects_real_implementation_with_forgotten_rejections(tmp_path):
    # Change the implementation, then execute real calls; do not fabricate bad events.
    source = (HERE / 'queue_store.py').read_text().replace(
        '            record["puts"][request_id] = (item, result)',
        '            if result:\n                record["puts"][request_id] = (item, result)')
    assert source != (HERE / 'queue_store.py').read_text()
    path = tmp_path / 'broken_queue.py'
    path.write_text(source)
    spec = importlib.util.spec_from_file_location('broken_queue', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with pytest.raises(SpecViolation, match='wrong operation result'):
        observe([7], command(0, True, 0), command(1, False), module.Queue)
