"""Contract faults chosen before inspecting driver-limit output."""


def variants(correct: str) -> dict[str, str]:
    foreign_release = correct.replace(
        "if (current == c.client) {", "if (current != 0) {", 1)
    resource_alias = correct.replace("c.resource in owners", "1 in owners").replace(
        "owners[c.resource]", "owners[1]")
    rejected_steal = correct.replace(
        "          accepted = true;\n        }\n      } else if (c.kind == 1)",
        "          accepted = true;\n        } else { owners[c.resource] = c.client; }\n      } else if (c.kind == 1)", 1)
    sparse_correct = correct.replace("owners[c.resource] = 0;", "owners -= (c.resource);", 1)
    dropped_release = correct.replace("if (current == c.client) {", "if (false) {", 1)
    return {"correct": correct, "correct_alt": sparse_correct,
            "foreign_release": foreign_release, "resource_alias": resource_alias,
            "rejected_acquire_steals": rejected_steal,
            "baseline_dropped_release": dropped_release}
