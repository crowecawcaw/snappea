"""Replays the shared conformance corpus (tests/conformance) through the Python API:
`emit(..., at=)` for events.jsonl, children files copied into the recording's events
folder, then `Recorder.check`. See tests/conformance/README.md."""

import json
import os
import shutil
import warnings

import pytest
from conftest import ROOT

from snappea import Recorder, SetupError, SpecViolation, emit
from snappea._recorder import _parse_time

CORPUS = os.path.join(ROOT, "tests", "conformance")
FEATURES = {"events_dir", "unknown_events"}
CASES = sorted(d for d in os.listdir(CORPUS) if os.path.isdir(os.path.join(CORPUS, d)))


def run_case(case: str, options: dict):
    """The case's outcome: None (pass), a SpecViolation or a SetupError, and the recorder."""
    rec = Recorder(
        options.get("specs", ()),
        spec_files=[os.path.join(case, "spec.p")],
        exclude=options.get("exclude"),
        strict=options.get("strict"),
        check_hot=options.get("check_hot"),
        unknown_events=options.get("unknown_events"),
    )
    with warnings.catch_warnings(record=True):
        warnings.simplefilter("always")
        try:
            with rec:
                with open(os.path.join(case, "events.jsonl"), encoding="utf-8") as f:
                    for line in f:
                        d = json.loads(line)
                        emit(d["event"], d.get("payload"), key=d["key"], at=_parse_time(d["timestamp"]))
                children = os.path.join(case, "children")
                if os.path.isdir(children):
                    for name in sorted(os.listdir(children)):
                        shutil.copy(os.path.join(children, name), rec.events_dir())
            rec.check()
            outcome = None
        except (SpecViolation, SetupError) as e:
            outcome = e
    return outcome, rec


@pytest.mark.parametrize("name", CASES)
def test_conformance(name):
    case = os.path.join(CORPUS, name)
    with open(os.path.join(case, "expect.json"), encoding="utf-8") as f:
        spec = json.load(f)
    missing = set(spec.get("requires", [])) - FEATURES
    if missing:
        pytest.skip(f"the Python helper does not support {', '.join(sorted(missing))}")
    expect = spec["expect"]
    outcome, rec = run_case(case, spec.get("options", {}))
    want = expect["outcome"]
    if want == "pass":
        assert outcome is None, f"expected a pass, got {outcome!r}"
    elif want == "violation":
        assert isinstance(outcome, SpecViolation), f"expected a violation, got {outcome!r}"
        assert outcome.spec == expect["spec"]
        if "key" in expect:
            assert outcome.key == expect["key"]
        assert expect["message"] in outcome.message
        assert outcome.liveness == expect.get("liveness", False)
    else:
        assert isinstance(outcome, SetupError), f"expected a setup error, got {outcome!r}"
        assert outcome.kind == expect["kind"], outcome.message
        assert expect.get("message", "") in outcome.message
    for spec_name, want_cov in expect.get("coverage", {}).items():
        report = rec.report or {}
        got = next(s["coverage"] for s in report.get("specs", []) if s["spec"] == spec_name)
        for part, counts in want_cov.items():
            for count, value in counts.items():
                assert got[part][count] == value, f"{spec_name} {part}.{count}"
