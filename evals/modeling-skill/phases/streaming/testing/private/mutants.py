"""Implementation variants for checking the private reference suite's sensitivity."""


def variants(correct):
    edits = {
        "fault_boundary_bucket": (
            "start = (timestamp // self.WIDTH) * self.WIDTH",
            "start = ((timestamp - 1) // self.WIDTH) * self.WIDTH if timestamp else 0",
        ),
        "fault_final_threshold_inclusive": (
            "self._watermark >= end + self.ALLOWED_LATENESS",
            "self._watermark > end + self.ALLOWED_LATENESS",
        ),
        "fault_negative_values": (
            'window["total"] += value',
            'window["total"] += abs(value)',
        ),
        "fault_late_correction_missing": (
            'if window["emitted"]:',
            'if window["emitted"] and False:',
        ),
        "fault_revision_does_not_advance": (
            'window["revision"] += 1',
            'window["revision"] += 0',
        ),
        "fault_late_first_arrival_silent": (
            "elif self._watermark >= end:",
            "elif False:",
        ),
        "fault_unsorted_outputs": (
            "for start in sorted(self._windows):",
            "for start in sorted(self._windows, reverse=True):",
        ),
    }
    result = {"correct": correct}
    for name, (before, after) in edits.items():
        if before not in correct:
            raise ValueError(f"cannot construct {name}: source anchor is missing")
        result[name] = correct.replace(before, after)
    final_missing = correct.replace(
        "if watermark >= end + self.ALLOWED_LATENESS:", "if False:")
    final_missing = final_missing.replace(
        "and watermark >= end + self.ALLOWED_LATENESS:", "and False:")
    if final_missing == correct:
        raise ValueError("cannot construct fault_final_records_missing")
    result["fault_final_records_missing"] = final_missing
    return result
