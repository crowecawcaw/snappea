"""Correct variants and deterministic fault controls for priority testing."""


def variants(correct):
    # Harmless representation variation: start the private admission counter at
    # another value. No public behavior exposes the counter itself.
    valid_counter_offset = correct.replace("self.__next_order = 0", "self.__next_order = 41", 1)
    faults = {
        "negative-priority": correct.replace(
            "self.__entries[item_id] = (priority, self.__next_order)",
            "self.__entries[item_id] = (abs(priority), self.__next_order)", 1),
        "lifo-tie": correct.replace(
            "self.__entries[key][1]))", "-self.__entries[key][1]))", 1),
        "reprioritize-renumbers": correct.replace(
            "self.__entries[item_id] = (priority, order)",
            "self.__entries[item_id] = (priority, self.__next_order)\n        self.__next_order += 1", 1),
        "reinsert-keeps-old-order": correct.replace(
            "        self.__next_order = 0",
            "        self.__next_order = 0\n        self.__old_orders = {}", 1).replace(
            "        self.__entries[item_id] = (priority, self.__next_order)",
            "        order = self.__old_orders.get(item_id, self.__next_order)\n"
            "        self.__entries[item_id] = (priority, order)\n        self.__old_orders[item_id] = order", 1),
        "duplicate-mutates": correct.replace(
            "        if item_id in self.__entries:\n            return False",
            "        if item_id in self.__entries:\n            self.__entries[item_id] = (priority, self.__next_order)\n"
            "            self.__next_order += 1\n            return False", 1),
        "empty-pop-value": correct.replace("            return None", "            return \"-\"", 1),
        "cancel-leaves-stale-entry": correct.replace(
            "    def cancel(self, item_id):\n        if item_id not in self.__entries:\n            return False\n"
            "        del self.__entries[item_id]\n        return True",
            "    def cancel(self, item_id):\n        if item_id not in self.__entries:\n            return False\n"
            "        return True", 1),
        "missing-reprioritize-adds": correct.replace(
            "        if item_id not in self.__entries:\n            return False\n        _, order = self.__entries[item_id]",
            "        if item_id not in self.__entries:\n            self.__entries[item_id] = (priority, self.__next_order)\n"
            "            self.__next_order += 1\n            return True\n        _, order = self.__entries[item_id]", 1),
    }
    result = {"correct": correct, "valid-counter-offset": valid_counter_offset, **faults}
    assert all(result[name] != correct for name in faults)
    assert len(set(result.values())) == len(result)
    return result
