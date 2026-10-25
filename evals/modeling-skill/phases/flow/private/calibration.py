"""Private implementation variants used only by the grader/calibrator."""


def variants(correct):
    lifo = correct.replace('record["items"].pop(0)', 'record["items"].pop()')
    retry = correct.replace(
        "            if result is not None:\n                return result[1]",
        "            if False and result is not None:\n                return result[1]", 1)
    retry = retry.replace(
        "            if request_id in record[\"takes\"]:\n                return record[\"takes\"][request_id]",
        "            if False and request_id in record[\"takes\"]:\n"
        "                return record[\"takes\"][request_id]", 1)
    cas = correct.replace(
        "            if self.store.compare_swap(version, record):\n                return accepted",
        "            self.store.compare_swap(version, record)\n            return accepted", 1)
    cas = cas.replace(
        "            if self.store.compare_swap(version, record):\n                return result",
        "            self.store.compare_swap(version, record)\n            return result", 1)
    return {"reference": correct, "case-01": lifo, "case-02": retry, "case-03": cas}
