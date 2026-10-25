import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from threading import BrokenBarrierError

from runtime import Store
from solution import Queue


class FirstReadBarrierStore:
    """Force two first loads to return the same version, then act as a real Store."""

    def __init__(self):
        self._store = Store()
        self._guard = threading.Lock()
        self._barrier = threading.Barrier(2, timeout=2)
        self._first_loads = 0
        self._armed = False
        self._forced = False

    def arm(self):
        with self._guard:
            self._armed = True

    @property
    def forced(self):
        with self._guard:
            return self._forced

    def load(self):
        result = self._store.load()
        wait = False
        with self._guard:
            if self._armed and self._first_loads < 2:
                self._first_loads += 1
                wait = True
        if wait:
            try:
                self._barrier.wait(timeout=2)
                with self._guard:
                    self._forced = True
            except BrokenBarrierError:
                pass
        return result

    def compare_swap(self, version, record):
        return self._store.compare_swap(version, record)


class QueueContractTests(unittest.TestCase):
    def test_fifo_capacity_and_rejection(self):
        store = Store()
        q = Queue(store)
        self.assertTrue(q.put("p1", 10))
        self.assertTrue(q.put("p2", 20))
        self.assertFalse(q.put("full", 30))
        self.assertEqual(q.snapshot(), [10, 20])
        self.assertEqual(q.take("t1"), 10)
        self.assertEqual(q.take("t2"), 20)
        self.assertEqual(q.take("empty"), None)
        self.assertEqual(q.snapshot(), [])

    def test_capacity_one_is_enforced(self):
        store = Store()
        q1, q2 = Queue(store, capacity=1), Queue(store, capacity=1)
        self.assertTrue(q1.put("only", 31))
        self.assertFalse(q2.put("extra", 32))
        self.assertEqual(q1.snapshot(), [31])
        self.assertEqual(q2.take("remove"), 31)
        self.assertTrue(q1.put("after-space", 33))
        self.assertEqual(q2.snapshot(), [33])

    def test_retried_put_does_not_append_again_after_dequeue(self):
        store = Store()
        q = Queue(store)
        self.assertTrue(q.put("same", 7))
        self.assertEqual(q.take("remove"), 7)
        self.assertTrue(q.put("same", 7))
        self.assertEqual(q.snapshot(), [])

    def test_empty_take_result_stays_memoized_after_later_put(self):
        store = Store()
        q = Queue(store)
        self.assertIsNone(q.take("empty-take"))
        self.assertTrue(q.put("later", 8))
        self.assertIsNone(q.take("empty-take"))
        self.assertEqual(q.snapshot(), [8])

    def test_rejected_put_stays_false_after_space_frees(self):
        store = Store()
        q = Queue(store)
        self.assertTrue(q.put("a", 1))
        self.assertTrue(q.put("b", 2))
        self.assertFalse(q.put("negative", 3))
        self.assertEqual(q.take("free"), 1)
        self.assertFalse(q.put("negative", 3))
        self.assertEqual(q.snapshot(), [2])

    def test_put_and_take_id_namespaces_are_separate(self):
        q = Queue(Store())
        self.assertTrue(q.put("same-id", 4))
        self.assertEqual(q.take("same-id"), 4)
        self.assertTrue(q.put("later", 5))
        self.assertEqual(q.take("same-id"), 4)
        self.assertEqual(q.snapshot(), [5])

    def test_request_memos_are_shared_across_instances(self):
        store = Store()
        first = Queue(store)
        second = Queue(store)
        self.assertTrue(first.put("p", 1))
        self.assertEqual(second.take("t"), 1)
        self.assertTrue(second.put("p", 1))
        self.assertEqual(first.snapshot(), [])
        self.assertEqual(first.take("t"), 1)
        self.assertEqual(second.snapshot(), [])

    def test_simultaneous_puts_retry_after_compare_swap_conflict(self):
        store = FirstReadBarrierStore()
        queue_a, queue_b = Queue(store), Queue(store)
        store.arm()
        with ThreadPoolExecutor(max_workers=2) as pool:
            a = pool.submit(queue_a.put, "a", 11)
            b = pool.submit(queue_b.put, "b", 22)
            self.assertTrue(a.result(timeout=4))
            self.assertTrue(b.result(timeout=4))
        self.assertCountEqual(queue_a.snapshot(), [11, 22])
        self.assertEqual(len(queue_a.snapshot()), 2)
        print(f"QUEUE_RACE_FORCED={int(store.forced)}")


if __name__ == "__main__":
    unittest.main()
