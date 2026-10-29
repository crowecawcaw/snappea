"""The P monitor checks events emitted by the implementation under test."""

import unittest
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier

from snappea import SpecViolation, check_spec

from storage import ReservationStore, assert_inventory_invariant


SPEC = str(Path(__file__).with_name("capacity.p"))


class ReservationTests(unittest.TestCase):
    def test_conditional_storage_serializes_competing_clients(self):
        with check_spec("Capacity", spec_files=[SPEC], strict=True, check_hot=True) as recorded:
            store = ReservationStore(1)
            first_read = store.read()
            second_read = store.read()

            self.assertTrue(store.reserve_if_unchanged("order-a", 1, first_read))
            self.assertFalse(store.reserve_if_unchanged("order-b", 1, second_read))
            assert_inventory_invariant(store)

        # These events came from storage.py calls above, not a hand-written trace.
        self.assertEqual(recorded.count("eReserved"), 1)
        reservation = next(event for event in recorded.events if event.event == "eReserved")
        self.assertTrue("storage.py:" in reservation.src, reservation.src)

    def test_rejections_preserve_state_and_do_not_mask_one_another(self):
        with check_spec("Capacity", spec_files=[SPEC], strict=True, check_hot=True):
            store = ReservationStore(2)
            old = store.read()
            self.assertTrue(store.reserve_if_unchanged("a", 1, old))
            current = store.read()

            # Capacity is still sufficient: only the stale-version condition fails.
            self.assertFalse(store.reserve_if_unchanged("b", 1, old))
            self.assertEqual(store.read(), current)
            self.assertEqual(store.reservations, {"a": 1})

            # Use a current snapshot to isolate insufficient capacity, then a
            # duplicate order. Check immediately so later writes cannot hide damage.
            self.assertFalse(store.reserve_if_unchanged("large", 2, current))
            self.assertEqual(store.read(), current)
            self.assertFalse(store.reserve_if_unchanged("a", 1, current))
            self.assertEqual(store.read(), current)
            self.assertEqual(store.reservations, {"a": 1})

            # Useful success prevents an always-rejecting implementation passing.
            self.assertTrue(store.reserve_if_unchanged("b", 1, current))
            assert_inventory_invariant(store)

    def test_forced_shared_snapshot_race_records_real_commits(self):
        with check_spec("Capacity", spec_files=[SPEC], strict=True, check_hot=True) as recorded:
            store = ReservationStore(1)
            both_read = Barrier(2)

            def reserve(order):
                snapshot = store.read()
                both_read.wait(timeout=5)  # both snapshots precede either write
                return store.reserve_if_unchanged(order, 1, snapshot)

            with ThreadPoolExecutor(max_workers=2) as workers:
                results = list(workers.map(reserve, ["a", "b"]))
            self.assertEqual(sum(results), 1)
            assert_inventory_invariant(store)

        # The observer checks committed capacity, which is independent of event
        # order. Do not use post-lock emission order as a general linearization oracle.
        self.assertEqual(recorded.count("eReserved"), 1)

    def test_unconditional_stale_write_breaks_the_same_monitor_and_oracle(self):
        store = None
        with self.assertRaisesRegex(SpecViolation, "reservations exceed available capacity"):
            with check_spec("Capacity", spec_files=[SPEC], strict=True, check_hot=True):
                store = ReservationStore(1)
                first_read = store.read()
                second_read = store.read()
                self.assertTrue(store.reserve_from_read_broken("order-a", 1, first_read))
                self.assertTrue(store.reserve_from_read_broken("order-b", 1, second_read))

        # Independent from the P monitor: recompute the invariant from the actual
        # committed rows and capacity, rather than trusting a result flag.
        with self.assertRaisesRegex(AssertionError, "reserved 2 units"):
            assert_inventory_invariant(store)


if __name__ == "__main__":
    unittest.main()
