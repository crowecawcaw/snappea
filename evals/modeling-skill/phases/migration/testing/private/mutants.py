"""Source variants for isolated evaluation of saved observation tests."""


def variants(correct_source: str) -> dict[str, str]:
    variants = {"correct": correct_source}
    variants["valid-same-revision-replay"] = correct_source.replace(
        "revision > current[0]", "revision >= current[0]", 1)
    variants["stale-backfill-overwrite"] = correct_source.replace(
        "current is None or revision > current[0]", "True", 1)
    variants["delete-drops-tombstone"] = correct_source.replace(
        "self.__legacy[key] = (revision, True, -1)\n        self.__shadow[key] = (revision, True, -1)",
        "self.__legacy[key] = (revision, True, -1)\n        self.__shadow.pop(key, None)", 1)
    variants["premature-cutover"] = correct_source.replace(
        "if not self.ready():\n            return False", "if False:\n            return False", 1)
    variants["missing-live-dual-write"] = correct_source.replace(
        "self.__legacy[key] = (revision, False, value)\n        self.__shadow[key] = (revision, False, 10 * value + 1)",
        "self.__legacy[key] = (revision, False, value)", 1)
    variants["wrong-prepare-watermark"] = correct_source.replace(
        "return token, watermark", "return token, watermark + 1", 1)
    variants["wrong-encoding"] = correct_source.replace(
        "10 * value + 1", "10 * value", 1)
    variants["read-returns-encoded-value"] = correct_source.replace(
        "(row[2] - 1) // 10 if self.__cutover else row[2]", "row[2]", 1)
    variants["delete-misses-shadow-tombstone"] = correct_source.replace(
        "self.__legacy[key] = (revision, True, -1)\n        self.__shadow[key] = (revision, True, -1)",
        "self.__legacy[key] = (revision, True, -1)", 1)
    variants["unknown-token-accepted"] = correct_source.replace(
        "if token not in self.__snapshots:\n            return False",
        "if token not in self.__snapshots:\n            return True", 1)
    return variants
