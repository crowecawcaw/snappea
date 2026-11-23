"""The `[tool.snappea]` keys match the Concepts table in docs/language-support.md."""

import os
import re

from conftest import ROOT

from snappea import _config


def concept_keys():
    with open(os.path.join(ROOT, "docs", "language-support.md"), encoding="utf-8") as f:
        section = f.read().split("### Concepts", 1)[1].split("\n#", 1)[0]
    return re.findall(r"^\| `(\w+)` \|", section, re.MULTILINE)


def test_config_keys_match_the_concepts_table():
    assert sorted(_config.KEYS) == sorted(concept_keys())
