import unittest

from snappea import check_events

from queue_service import Service


class ObservedQueueTests(unittest.TestCase):
    def setUp(self):
        self.service = Service()
        self.events = []

    def submit(self, item_id, priority):
        accepted = self.service.submit(item_id, priority)
        self.events.append({"event": "eSubmit", "item_id": item_id,
                            "priority": priority, "accepted": accepted})
        return accepted

    def pop(self):
        item_id = self.service.pop()
        self.events.append({"event": "ePop", "item_id": "-" if item_id is None else item_id})
        return item_id

    def reprioritize(self, item_id, priority):
        accepted = self.service.reprioritize(item_id, priority)
        self.events.append({"event": "eReprioritize", "item_id": item_id,
                            "priority": priority, "accepted": accepted})
        return accepted

    def cancel(self, item_id):
        accepted = self.service.cancel(item_id)
        self.events.append({"event": "eCancel", "item_id": item_id,
                            "accepted": accepted})
        return accepted

    def monitor(self):
        check_events(self.events, "QueueContract",
                     spec_files=["events.p", "contract.p"], strict=True,
                     unknown_events="error")

    def test_signed_priorities_and_fifo_ties(self):
        self.assertIsNone(self.pop())
        self.assertTrue(self.submit("middle", 3))
        self.assertTrue(self.submit("first", -2))
        self.assertTrue(self.submit("second", -2))
        self.assertTrue(self.submit("last", 0))
        self.assertEqual(self.pop(), "first")
        self.assertEqual(self.pop(), "second")
        self.assertEqual(self.pop(), "last")
        self.assertEqual(self.pop(), "middle")
        self.assertIsNone(self.pop())
        self.monitor()

    def test_reprioritize_keeps_admission_order(self):
        self.assertTrue(self.submit("early", 8))
        self.assertTrue(self.submit("later", 8))
        self.assertTrue(self.reprioritize("early", 8))
        self.assertFalse(self.reprioritize("missing", -100))
        self.assertEqual(self.pop(), "early")
        self.assertEqual(self.pop(), "later")
        self.monitor()

    def test_reprioritize_changes_priority_without_losing_stability(self):
        self.assertTrue(self.submit("a", 4))
        self.assertTrue(self.submit("b", 1))
        self.assertTrue(self.submit("c", 3))
        self.assertTrue(self.reprioritize("a", -5))
        self.assertEqual(self.pop(), "a")
        self.assertEqual(self.pop(), "b")
        self.assertEqual(self.pop(), "c")
        self.monitor()

    def test_cancel_and_reinsert_gets_fresh_order(self):
        self.assertTrue(self.submit("a", 0))
        self.assertTrue(self.submit("b", 0))
        self.assertTrue(self.cancel("a"))
        self.assertFalse(self.cancel("a"))
        self.assertTrue(self.submit("a", 0))
        self.assertEqual(self.pop(), "b")
        self.assertEqual(self.pop(), "a")
        self.monitor()

    def test_duplicate_rejection_does_not_change_entry(self):
        self.assertTrue(self.submit("a", 5))
        self.assertTrue(self.submit("b", 5))
        self.assertFalse(self.submit("a", 90))
        self.assertEqual(self.pop(), "a")
        self.assertEqual(self.pop(), "b")
        self.monitor()

    def test_cancelled_item_never_reappears(self):
        self.assertTrue(self.submit("discard", -10))
        self.assertTrue(self.cancel("discard"))
        self.assertTrue(self.submit("keep", 2))
        self.assertEqual(self.pop(), "keep")
        self.assertIsNone(self.pop())
        self.monitor()


if __name__ == "__main__":
    unittest.main()
