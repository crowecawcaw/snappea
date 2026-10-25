"""The P monitor checks events emitted by the implementation under test."""

import unittest

from snappea import SpecViolation, check_spec

from storage import ReservationStore, assert_inventory_invariant


SPEC = "capacity.p"


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
