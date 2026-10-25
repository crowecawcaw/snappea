"""Deterministic tumbling-window aggregation with event-time watermarks."""


class Service:
    WIDTH = 10
    ALLOWED_LATENESS = 5

    def __init__(self):
        self._watermark = 0
        self._windows = {}

    def add(self, timestamp, value):
        if not isinstance(timestamp, int) or isinstance(timestamp, bool) or timestamp < 0:
            raise ValueError("timestamp must be a nonnegative integer")
        if not isinstance(value, int) or isinstance(value, bool):
            raise ValueError("value must be an integer")
        start = (timestamp // self.WIDTH) * self.WIDTH
        end = start + self.WIDTH
        if self._watermark >= end + self.ALLOWED_LATENESS:
            return False, []

        window = self._windows.get(start)
        if window is None:
            window = {"total": 0, "revision": -1, "emitted": False, "final": False}
            self._windows[start] = window
        window["total"] += value

        outputs = []
        if window["emitted"]:
            window["revision"] += 1
            outputs.append(self._record(start, window, final=False))
        elif self._watermark >= end:
            window["emitted"] = True
            window["revision"] = 0
            outputs.append(self._record(start, window, final=False))
        return True, outputs

    def advance(self, watermark):
        if not isinstance(watermark, int) or isinstance(watermark, bool) or watermark < self._watermark:
            raise ValueError("watermark must be a nondecreasing integer")
        self._watermark = watermark
        outputs = []
        for start in sorted(self._windows):
            window = self._windows[start]
            end = start + self.WIDTH
            if not window["emitted"] and watermark >= end:
                window["emitted"] = True
                if watermark >= end + self.ALLOWED_LATENESS:
                    window["revision"] = 0
                    window["final"] = True
                    outputs.append(self._record(start, window, final=True))
                else:
                    window["revision"] = 0
                    outputs.append(self._record(start, window, final=False))
            elif window["emitted"] and not window["final"] and watermark >= end + self.ALLOWED_LATENESS:
                window["revision"] += 1
                window["final"] = True
                outputs.append(self._record(start, window, final=True))
        return outputs

    @staticmethod
    def _record(start, window, final):
        return {
            "start": start,
            "end": start + 10,
            "total": window["total"],
            "revision": window["revision"],
            "final": final,
        }
