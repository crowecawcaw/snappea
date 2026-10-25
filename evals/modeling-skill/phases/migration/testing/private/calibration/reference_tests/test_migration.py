import unittest

from snappea import check_events

from migration import Service


SPEC = "contract.p"


class ObservedMigrationTests(unittest.TestCase):
    def setUp(self):
        self.service = Service()
        self.events = []

    def monitor(self):
        check_events(self.events, "MigrationContract", spec_files=["events.p", SPEC],
                     strict=True, unknown_events="error")

    def prepare(self):
        token, watermark = self.service.prepare()
        self.events.append({"event": "ePrepare", "token": token, "watermark": watermark})
        return token, watermark

    def apply(self, token):
        applied = self.service.apply(token)
        self.events.append({"event": "eApply", "token": token, "applied": applied})
        return applied

    def write(self, key, value):
        revision = self.service.write(key, value)
        self.events.append({"event": "eWrite", "item": key, "value": value,
                            "revision": revision})
        return revision

    def delete(self, key):
        revision = self.service.delete(key)
        self.events.append({"event": "eDelete", "item": key, "revision": revision})
        return revision

    def cutover(self):
        accepted = self.service.cutover()
        self.events.append({"event": "eCutover", "accepted": accepted})
        return accepted

    def read(self, key):
        present, value = self.service.read(key)
        self.events.append({"event": "eRead", "item": key, "present": present,
                            "value": value})
        return present, value

    def inspect_new(self, key):
        revision, deleted, encoded = self.service.inspect_new(key)
        self.events.append({"event": "eNewRow", "item": key, "revision": revision,
                            "deleted": deleted, "encoded": encoded})
        return revision, deleted, encoded

    def ready(self):
        ready = self.service.ready()
        self.events.append({"event": "eReady", "complete": ready})
        return ready

    def test_stale_snapshot_cannot_overwrite_live_write_or_resurrect_delete(self):
        token, watermark = self.prepare()
        self.assertEqual((token, watermark), (1, 0))
        self.assertEqual(self.write("a", 31), 1)
        self.assertEqual(self.delete("b"), 2)
        self.assertTrue(self.apply(token))
        self.assertEqual(self.inspect_new("a"), (1, False, 311))
        self.assertEqual(self.inspect_new("b"), (2, True, -1))
        self.assertEqual(self.read("a"), (True, 31))
        self.assertEqual(self.read("b"), (False, 0))
        self.assertTrue(self.ready())
        self.assertTrue(self.cutover())
        self.assertEqual(self.read("a"), (True, 31))
        self.assertEqual(self.read("b"), (False, 0))
        self.monitor()

    def test_cutover_waits_for_initial_backfill_and_reads_survive_transition(self):
        self.assertEqual(self.read("a"), (True, 10))
        self.assertFalse(self.ready())
        self.assertFalse(self.cutover())
        token, watermark = self.prepare()
        self.assertEqual((token, watermark), (1, 0))
        self.assertTrue(self.apply(token))
        self.assertEqual(self.inspect_new("a"), (0, False, 101))
        self.assertEqual(self.inspect_new("b"), (0, False, 201))
        self.assertTrue(self.ready())
        self.assertEqual(self.read("a"), (True, 10))
        self.assertTrue(self.cutover())
        self.assertEqual(self.read("a"), (True, 10))
        self.assertEqual(self.write("a", 12), 1)
        self.assertEqual(self.read("a"), (True, 12))
        self.assertEqual(self.inspect_new("a"), (1, False, 121))
        self.assertEqual(self.delete("a"), 2)
        self.assertEqual(self.read("a"), (False, 0))
        self.assertEqual(self.inspect_new("a"), (2, True, -1))
        self.assertTrue(self.ready())
        self.monitor()

    def test_each_prepare_is_an_immutable_reusable_snapshot(self):
        first, watermark = self.prepare()
        self.assertEqual(watermark, 0)
        self.assertTrue(self.apply(first))
        self.assertEqual(self.write("c", 7), 1)
        second, watermark = self.prepare()
        self.assertEqual((second, watermark), (2, 1))
        self.assertEqual(self.write("c", 8), 2)
        self.assertTrue(self.apply(second))
        self.assertEqual(self.inspect_new("c"), (2, False, 81))
        self.assertFalse(self.apply(first))
        self.assertEqual(self.inspect_new("c"), (2, False, 81))
        self.assertFalse(self.apply(999))
        self.assertTrue(self.ready())
        self.monitor()


if __name__ == "__main__":
    unittest.main()
