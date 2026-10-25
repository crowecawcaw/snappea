import unittest

from snappea import check_events

from lease import Service


SPEC = "lease_contract.p"


class ObservedLeaseTests(unittest.TestCase):
    def setUp(self):
        self.service = Service()
        self.events = []

    def monitor(self):
        check_events(self.events, "LeaseContract", spec_files=["events.p", SPEC], strict=True,
                     unknown_events="error")

    def acquire(self, client, ttl):
        token = self.service.acquire(client, ttl)
        self.events.append({"event": "eAcquire", "client": client, "ttl": ttl,
                            "token": -1 if token is None else token})
        return token

    def write(self, client, token, value):
        accepted = self.service.write(client, token, value)
        self.events.append({"event": "eWrite", "client": client, "token": token,
                            "value": value, "accepted": accepted})
        return accepted

    def release(self, client, token):
        accepted = self.service.release(client, token)
        self.events.append({"event": "eRelease", "client": client, "token": token,
                            "accepted": accepted})
        return accepted

    def advance(self, ticks):
        self.service.advance(ticks)
        self.events.append({"event": "eAdvance", "ticks": ticks})

    def read(self):
        value = self.service.read()
        self.events.append({"event": "eRead", "value": value})
        return value

    def peek(self):
        result = self.service.peek()
        owner, token, expires_at = result if result is not None else ("-", -1, -1)
        self.events.append({"event": "ePeek", "owner": owner, "token": token,
                            "expires_at": expires_at})
        return result

    def test_success_rejection_and_monotonic_reacquire(self):
        first = self.acquire("writer", 4)
        self.assertIsInstance(first, int)
        self.assertGreater(first, 0)
        self.assertEqual(self.peek(), ("writer", first, 4))
        self.assertIsNone(self.acquire("writer", 4))
        self.assertIsNone(self.acquire("other", 4))
        self.assertFalse(self.write("other", first, 8))
        self.assertEqual(self.read(), 0)
        self.assertTrue(self.write("writer", first, 7))
        self.assertEqual(self.read(), 7)
        self.assertTrue(self.release("writer", first))
        self.assertIsNone(self.peek())

        second = self.acquire("writer", 3)
        self.assertIsInstance(second, int)
        self.assertGreater(second, first)
        self.assertFalse(self.write("writer", first, 99))
        self.assertEqual(self.read(), 7)
        self.assertEqual(self.read(), 7)
        self.assertTrue(self.write("writer", second, 9))
        self.assertEqual(self.read(), 9)
        self.monitor()

    def test_exact_expiry_allows_successor_and_rejects_expired_write(self):
        first = self.acquire("a", 3)
        self.assertIsInstance(first, int)
        self.assertGreater(first, 0)
        self.assertEqual(self.peek(), ("a", first, 3))
        self.advance(2)
        self.assertEqual(self.peek(), ("a", first, 3))
        self.advance(1)
        self.assertIsNone(self.peek())
        self.assertFalse(self.write("a", first, 12))
        self.assertEqual(self.read(), 0)

        second = self.acquire("b", 2)
        self.assertGreater(second, first)
        self.assertEqual(self.peek(), ("b", second, 5))
        self.assertTrue(self.write("b", second, 5))
        self.assertEqual(self.read(), 5)
        self.monitor()

    def test_stale_release_cannot_clear_successor_lease(self):
        stale = self.acquire("a", 1)
        self.advance(1)
        current = self.acquire("b", 4)
        self.assertIsInstance(current, int)
        self.assertGreater(current, stale)
        self.assertFalse(self.release("a", stale))
        self.assertEqual(self.peek(), ("b", current, 5))
        self.assertTrue(self.write("b", current, 21))
        self.assertEqual(self.read(), 21)
        self.monitor()


if __name__ == "__main__":
    unittest.main()
