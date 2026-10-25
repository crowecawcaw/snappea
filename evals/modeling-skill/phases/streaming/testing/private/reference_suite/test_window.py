"""Exercise Service and check the saved public-result history with Snappea."""

import json
import subprocess
import tempfile
import unittest

from window import Service


class WindowObservationTests(unittest.TestCase):
    def test_window_lateness_corrections_and_finalization(self):
        service = Service()
        observed = []

        def close_call():
            observed.append({"event": "eEndCall"})

        def add(timestamp, value, expected):
            accepted, outputs = service.add(timestamp, value)
            self.assertEqual((accepted, outputs), expected)
            observed.append({"event": "eAdd", "payload": {
                "timestamp": timestamp, "value": value, "accepted": accepted}})
            self._outputs(observed, outputs)
            close_call()

        def advance(watermark, expected):
            outputs = service.advance(watermark)
            self.assertEqual(outputs, expected)
            observed.append({"event": "eWatermark", "payload": {"watermark": watermark}})
            self._outputs(observed, outputs)
            close_call()

        add(0, 7, (True, []))
        add(10, -2, (True, []))
        advance(10, [{"start": 0, "end": 10, "total": 7, "revision": 0, "final": False}])
        add(9, 3, (True, [{"start": 0, "end": 10, "total": 10, "revision": 1, "final": False}]))
        add(19, 4, (True, []))
        advance(15, [{"start": 0, "end": 10, "total": 10, "revision": 2, "final": True}])
        add(1, 100, (False, []))
        advance(20, [{"start": 10, "end": 20, "total": 2, "revision": 0, "final": False}])
        add(10, -5, (True, [{"start": 10, "end": 20, "total": -3, "revision": 1, "final": False}]))
        advance(25, [{"start": 10, "end": 20, "total": -3, "revision": 2, "final": True}])
        self._check_observation(observed)

    def test_late_first_event_and_simultaneous_final_order(self):
        service = Service()
        observed = []
        for timestamp, value in ((1, 3), (11, -4)):
            accepted, outputs = service.add(timestamp, value)
            self.assertEqual((accepted, outputs), (True, []))
            observed.append({"event": "eAdd", "payload": {
                "timestamp": timestamp, "value": value, "accepted": accepted}})
            observed.append({"event": "eEndCall"})
        outputs = service.advance(25)
        expected = [
            {"start": 0, "end": 10, "total": 3, "revision": 0, "final": True},
            {"start": 10, "end": 20, "total": -4, "revision": 0, "final": True},
        ]
        self.assertEqual(outputs, expected)
        observed.append({"event": "eWatermark", "payload": {"watermark": 25}})
        self._outputs(observed, outputs)
        observed.append({"event": "eEndCall"})
        self._check_observation(observed)

        late_service = Service()
        late_observed = []
        self.assertEqual(late_service.advance(12), [])
        late_observed.extend([
            {"event": "eWatermark", "payload": {"watermark": 12}},
            {"event": "eEndCall"},
        ])
        accepted, outputs = late_service.add(2, -7)
        expected_late = [{"start": 0, "end": 10, "total": -7, "revision": 0, "final": False}]
        self.assertEqual((accepted, outputs), (True, expected_late))
        late_observed.append({"event": "eAdd", "payload": {
            "timestamp": 2, "value": -7, "accepted": accepted}})
        self._outputs(late_observed, outputs)
        late_observed.append({"event": "eEndCall"})
        self._check_observation(late_observed)

    def test_direct_jump_emits_one_final_record(self):
        service = Service()
        observed = []
        accepted, outputs = service.add(74, 6)
        self.assertTrue(accepted)
        self.assertEqual(outputs, [])
        observed.append({"event": "eAdd", "payload": {"timestamp": 74, "value": 6, "accepted": accepted}})
        observed.append({"event": "eEndCall"})
        outputs = service.advance(85)
        self.assertEqual(outputs, [{"start": 70, "end": 80, "total": 6, "revision": 0, "final": True}])
        observed.append({"event": "eWatermark", "payload": {"watermark": 85}})
        self._outputs(observed, outputs)
        observed.append({"event": "eEndCall"})
        self._check_observation(observed)

    @staticmethod
    def _outputs(events, outputs):
        for item in outputs:
            events.append({"event": "eOutput", "payload": {
                "window_start": item["start"], "window_end": item["end"],
                "total": item["total"], "revision": item["revision"], "final": item["final"]}})

    def _check_observation(self, events):
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", suffix=".jsonl") as log:
            for sequence, item in enumerate(events):
                item["timestamp"] = sequence
                item["key"] = "windows"
                log.write(json.dumps(item, separators=(",", ":")) + "\n")
            log.flush()
            result = subprocess.run(
                ["snappea", "observe", "events.p", "contract.p", log.name,
                 "--spec", "WindowContract", "--strict", "--unknown-events", "error"],
                text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
