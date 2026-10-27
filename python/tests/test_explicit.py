"""Explicit mode: check_spec, expect_violation, check_events."""

import os
import re
import subprocess
import sys
import threading
import warnings
from datetime import datetime, timezone

import pytest
from app.locks import LockServer
from conftest import SPEC

from snappea import Event, Recorder, SetupError, SpecViolation, check_events, check_spec, emit, expect_violation

FILES = [SPEC]


class Clock:
    def __init__(self):
        self.t = 0.0

    def __call__(self):
        return self.t


def test_correct_server_passes():
    clock = Clock()
    with check_spec("MutualExclusion", spec_files=FILES) as rec:
        server = LockServer(lease=1, clock=clock)
        server.acquire("c1")
        clock.t = 2
        server.acquire("c2")
    assert rec.events_named("eEnter") == [{"client": "c1"}, {"client": "c2"}]
    assert rec.count("eExit") == 1


def test_lease_bug_is_reported_on_the_with_line():
    clock = Clock()
    with pytest.raises(SpecViolation) as info:
        with check_spec("MutualExclusion", spec_files=FILES):
            server = LockServer(lease=1, clock=clock, buggy_expiry=True)
            server.acquire("c1")
            clock.t = 2
            server.acquire("c2")
    v = info.value
    assert isinstance(v, AssertionError)
    assert v.spec == "MutualExclusion"
    assert v.message == "c2 entered while c1 is inside"
    assert v.key == "L1"
    assert [e.event for e in v.events] == ["eEnter", "eEnter"]
    assert v.events[-1].payload == {"client": "c2"}
    assert v.events[-1].src.endswith("locks.py:27")
    assert v.location == os.path.relpath(SPEC) + ":19"
    text = str(v)
    assert text.startswith("P spec MutualExclusion was broken: c2 entered while c1 is inside")
    assert "<- broke the spec here" in text
    assert os.path.isdir(v.output_dir)


def test_all_specs_by_default_and_check_hot_is_opt_in():
    server = LockServer()
    with check_spec(spec_files=FILES):
        server.acquire("c1")
        server.acquire("c2")  # queued: never granted
    with pytest.raises(SpecViolation) as info:
        with check_spec(spec_files=FILES, check_hot=True):
            server.acquire("c3")
    assert info.value.spec == "RequestsGranted"
    assert info.value.liveness
    assert "ended in hot state 'Waiting'" in str(info.value)
    assert "<- events ended here" in str(info.value)


def test_keys_are_independent_streams():
    with check_spec("MutualExclusion", spec_files=FILES):
        emit("eEnter", key="L1", client="c1")
        emit("eEnter", key="L2", client="c2")


def test_expect_violation_on_emitted_events():
    with expect_violation("MutualExclusion", match="c2 entered", spec_files=FILES) as ev:
        emit("eEnter", client="c1")
        emit("eEnter", client="c2")
    assert ev.violation.message == "c2 entered while c1 is inside"


def test_expect_violation_fails_when_the_spec_holds():
    with pytest.raises(AssertionError, match="expected P spec MutualExclusion to be broken, but it held over 1 event"):
        with expect_violation("MutualExclusion", spec_files=FILES):
            emit("eEnter", client="c1")


def test_expect_violation_checks_the_message():
    with pytest.raises(AssertionError, match="does not match 'c3'"):
        with expect_violation("MutualExclusion", match="c3", spec_files=FILES):
            emit("eEnter", client="c1")
            emit("eEnter", client="c2")


def test_check_events_tests_the_spec_itself():
    with expect_violation("MutualExclusion", match="c2 entered", spec_files=FILES):
        check_events(
            [
                {"event": "eEnter", "payload": {"client": "c1"}},
                {"event": "eEnter", "payload": {"client": "c2"}},
            ],
            "MutualExclusion",
            spec_files=FILES,
        )
    check_events(
        [{"event": "eEnter", "client": "c1"}, {"event": "eExit", "client": "c1"}],
        "MutualExclusion",
        spec_files=FILES,
    )


def test_check_events_reports_list_positions():
    with pytest.raises(SpecViolation) as info:
        check_events(
            [{"event": "eEnter", "client": "c1"}, {"event": "eExit", "client": "c2"}],
            "MutualExclusion",
            spec_files=FILES,
        )
    assert info.value.events[-1].src == "events[1]"
    assert info.value.message == "c2 left, but c1 holds the lock"


def test_nested_recorders_both_see_events():
    with check_spec("MutualExclusion", spec_files=FILES) as outer:
        emit("eEnter", client="c1")
        emit("eExit", client="c1")
        with check_spec("MutualExclusion", spec_files=FILES) as inner:
            emit("eEnter", client="c2")
            emit("eExit", client="c2")
        assert inner.count("eEnter") == 1
    assert outer.count("eEnter") == 2 and outer.count("eExit") == 2


def test_at_overrides_the_time():
    t0 = datetime(2026, 10, 18, tzinfo=timezone.utc)
    with check_spec("MutualExclusion", spec_files=FILES) as rec:
        # logged out of order; times say exit came first
        emit("eEnter", client="c2", at=t0.replace(second=2))
        emit("eExit", client="c1", at=t0.replace(second=1))
        emit("eEnter", client="c1", at=t0)
    assert [e.event for e in rec.events] == ["eEnter", "eExit", "eEnter"]
    with pytest.raises(SpecViolation) as info:
        with check_spec("MutualExclusion", spec_files=FILES):
            emit("eEnter", client="c1", at=2)
            emit("eEnter", client="c2", at=1)  # a logical clock: c2 entered first
    assert info.value.message == "c1 entered while c2 is inside"
    assert "(time set with at=)" in str(info.value)


def test_payload_mismatch_is_a_clear_error():
    with pytest.raises(SetupError) as info:
        with check_spec("MutualExclusion", spec_files=FILES):
            emit("eEnter", who="c1")
    msg = str(info.value)
    assert "do not match their declaration" in msg
    assert "missing field 'client'" in msg
    assert "test_explicit.py" in msg
    assert info.value.kind == "parser"


def test_unknown_spec_is_a_clear_error():
    with pytest.raises(SetupError, match="no spec named NoSuchSpec. Specs: MutualExclusion") as info:
        with check_spec("NoSuchSpec", spec_files=FILES):
            emit("eEnter", client="c1")
    assert info.value.kind == "config"
    assert "--help" not in str(info.value)


def test_p_source_error_is_a_clear_error(tmp_path):
    # snappea stops before checking: its JSON result has an error and no specs
    bad = tmp_path / "bad.p"
    bad.write_text("event eEnter: (client: string);\nspec Broken observes eEnter {\n")
    with pytest.raises(SetupError, match="does not type-check") as info:
        check_events([{"event": "eEnter", "client": "c1"}], spec_files=[str(bad)])
    assert info.value.kind == "config"
    assert "bad.p" in str(info.value)


@pytest.mark.skipif(os.name == "nt", reason="the fake snappea is a shell script")
@pytest.mark.parametrize(
    "output, says",
    [
        ("Failed parsing ::\nWas passed main parameter 'x.p' but no main parameter was defined\nUsage: p observe", "old PObserve-style"),
        ("error: unknown option '--json-report'\nRun `snappea observe --help` for usage.", "does not support --json-report"),
    ],
)
def test_outdated_binary_is_a_clear_error(tmp_path, monkeypatch, output, says):
    from snappea import _binary

    fake = tmp_path / "snappea"
    fake.write_text(f"#!/bin/sh\ncat <<'EOF'\n{output}\nEOF\nexit 2\n")
    fake.chmod(0o755)
    monkeypatch.setenv("SNAPPEA_BIN", str(fake))
    _binary.find_snappea.cache_clear()
    try:
        with pytest.raises(SetupError, match=f"{says}.*too old for this snappea package") as info:
            check_events([{"event": "eEnter", "client": "c1"}], spec_files=FILES)
        assert info.value.kind == "config"
    finally:
        _binary.find_snappea.cache_clear()


def test_spec_files_take_globs_and_folders():
    folder = os.path.relpath(os.path.dirname(SPEC))
    for spec_files in ([folder], [os.path.join(folder, "*.p")]):
        check_events([{"event": "eEnter", "client": "c1"}], "MutualExclusion", spec_files=spec_files)
    with pytest.raises(SetupError, match="matches no files") as info:
        check_events([{"event": "eEnter", "client": "c1"}], spec_files=["nope/*.p"])
    assert info.value.kind == "config"


def test_no_events_skips_the_check():
    with check_spec(spec_files=["nope.p"]):
        pass
    with check_spec("NoSuchSpec", spec_files=["nope.p"], strict=False):
        pass


def test_events_from_threads_are_kept():
    with expect_violation("MutualExclusion", spec_files=FILES):
        t = threading.Thread(target=lambda: emit("eEnter", client="t1"))
        t.start()
        t.join()
        emit("eEnter", client="main")


def test_events_from_child_processes(monkeypatch):
    code = "from snappea import emit; emit('eEnter', key='L9', client='child')"
    monkeypatch.setenv("PYTHONPATH", os.pathsep.join(sys.path))
    with expect_violation("MutualExclusion", match="parent entered while child", spec_files=FILES) as ev:
        subprocess.run([sys.executable, "-c", code], check=True)  # inherits SNAPPEA_EVENTS_DIR
        emit("eEnter", key="L9", client="parent")
    assert ev.violation.events[0].src == "<string>:1"
    assert ev.violation.events[0].pid != os.getpid()


def test_existing_logs(tmp_path):
    log = tmp_path / "server.log"
    log.write_text(
        "2026-10-18T00:00:01Z INFO granted lock L1 to c1\n"
        "2026-10-18T00:00:02Z INFO heartbeat\n"
        "2026-10-18T00:00:03Z INFO granted lock L1 to c2\n"
    )

    def parse(line):
        parts = line.split()
        if parts[2:4] != ["granted", "lock"]:
            return None
        return {"event": "eEnter", "key": parts[4], "at": datetime.fromisoformat(parts[0].replace("Z", "+00:00")),
                "client": parts[6]}

    rec = Recorder(["MutualExclusion"], spec_files=FILES)
    rec.add_log(str(log), parse)
    assert rec.count("eEnter") == 2
    with pytest.raises(SpecViolation) as info:
        rec.check()
    assert info.value.events[-1].src == f"{log}:3"


def test_positional_payloads():
    with check_spec("MutualExclusion", spec_files=FILES) as rec:
        emit("eEnter", {"client": "c1"}, key="L1")
    assert rec.events_named("eEnter") == [{"client": "c1"}]
    with check_spec("MutualExclusion", spec_files=FILES, strict=False), pytest.raises(TypeError):
        emit("eEnter", {"client": "c1"}, client="c1")


@pytest.mark.skipif(not hasattr(os, "fork"), reason="needs fork")
def test_events_from_forked_processes():
    import multiprocessing

    ctx = multiprocessing.get_context("fork")
    with expect_violation("MutualExclusion", match="parent entered while forked", spec_files=FILES):
        p = ctx.Process(target=emit, args=("eEnter",), kwargs={"key": "F", "client": "forked"})
        p.start()
        p.join()
        emit("eEnter", key="F", client="parent")


def test_strict_defaults_to_true_for_named_specs():
    with warnings.catch_warnings():
        warnings.simplefilter("error")  # fails now: no StrictDefaultWarning grace period
        with pytest.raises(SetupError, match="checked no relevant events") as info:
            with check_spec("MutualExclusion", spec_files=FILES):
                pass
    assert info.value.kind == "validation"
    assert "strict=False" in info.value.message  # the way out when no events is intended
    with pytest.raises(SetupError, match="RequestsGranted") as info:
        with check_spec("MutualExclusion", "RequestsGranted", spec_files=FILES):
            emit("eEnter", client="a")
    assert info.value.kind == "validation"
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        with check_spec("MutualExclusion", spec_files=FILES):
            emit("eEnter", client="a")
        with check_spec(spec_files=FILES):  # every spec: not strict
            emit("eEnter", client="a")


def test_strict_false_allows_named_specs_without_events():
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        with check_spec("MutualExclusion", spec_files=FILES, strict=False):
            pass
        with check_spec("MutualExclusion", "RequestsGranted", spec_files=FILES, strict=False):
            emit("eEnter", client="a")


def test_strict_true_fails_without_relevant_events():
    with pytest.raises(SetupError, match="checked no relevant events") as info:
        with check_spec("MutualExclusion", spec_files=FILES, strict=True):
            pass
    assert info.value.kind == "validation"
    with check_spec("MutualExclusion", spec_files=FILES, strict=True):
        emit("eEnter", client="a")
    with pytest.raises(SetupError, match="RequestsGranted.*checked no relevant events"):
        check_events([{"event": "eEnter", "payload": {"client": "a"}}], spec_files=FILES, strict=True)


def test_strict_keeps_sibling_safety_violation_primary():
    with pytest.raises(SpecViolation, match="entered while"):
        with check_spec(spec_files=FILES, strict=True):
            emit("eEnter", client="a")
            emit("eEnter", client="b")


def test_misspelled_event_is_an_error_by_default():
    with pytest.raises(SetupError) as info:
        with check_spec("MutualExclusion", spec_files=FILES):
            emit("eEnter", client="c1")
            emit("eExitt", client="c1")
    e = info.value
    assert e.kind == "unknown_event"
    assert re.search(r"eExitt: 1 time\(s\), first at \S*test_explicit.py:\d+", e.message)
    assert "did you mean eExit?" in e.message


def test_unknown_events_warn_and_ignore():
    with pytest.warns(UserWarning, match="eExitt.*test_explicit.py:.*did you mean eExit"):
        with check_spec("MutualExclusion", spec_files=FILES, unknown_events="warn"):
            emit("eEnter", client="c1")
            emit("eExitt", client="c1")
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        check_events([{"event": "eEnter", "client": "c1"}, {"event": "eNope"}], spec_files=FILES, unknown_events="ignore")
    with pytest.raises(SetupError, match="unknown_events must be one of ignore, warn, error"):
        check_spec(spec_files=FILES, unknown_events="loud")


def test_liveness_is_a_deprecated_alias_of_check_hot():
    with pytest.warns(DeprecationWarning, match="liveness= is deprecated; use check_hot="):
        spec = check_spec(spec_files=FILES, liveness=True)
    assert spec.recorder.check_hot is True
    with pytest.raises(SpecViolation) as info, pytest.warns(DeprecationWarning):
        check_events([{"event": "eRequest", "client": "c1"}], spec_files=FILES, liveness=True)
    assert info.value.liveness
    with pytest.raises(TypeError):
        Recorder(check_hot=True, liveness=True)


def test_setup_error_kinds_and_old_names(monkeypatch, tmp_path):
    from snappea import _binary

    with pytest.warns(DeprecationWarning, match="snappea.SnappeaError is deprecated"):
        from snappea import SnappeaError
    assert SnappeaError is SetupError
    assert issubclass(_binary.SnappeaNotFound, SetupError)
    binary = _binary.find_snappea()
    try:
        monkeypatch.setenv("SNAPPEA_BIN", str(tmp_path / "missing"))
        _binary.find_snappea.cache_clear()
        with pytest.raises(SetupError, match="SNAPPEA_BIN=.*missing is not a file") as info:
            check_events([{"event": "eEnter", "client": "c1"}], spec_files=FILES)
        assert info.value.kind == "binary_missing"
        monkeypatch.delenv("SNAPPEA_BIN")
        monkeypatch.setenv("SNAPPEA_BINARY", binary)
        _binary.find_snappea.cache_clear()
        with pytest.warns(DeprecationWarning, match="SNAPPEA_BINARY is deprecated; set SNAPPEA_BIN"):
            assert _binary.find_snappea() == binary
    finally:
        _binary.find_snappea.cache_clear()


def test_timeout_is_a_setup_error():
    with pytest.raises(SetupError, match="did not finish within") as info:
        check_events([{"event": "eEnter", "client": "c1"}], spec_files=FILES, timeout=1e-6)
    assert info.value.kind == "timeout"


def test_artifacts_are_kept_only_for_failed_checks(tmp_path, monkeypatch):
    check_events([{"event": "eEnter", "client": "c1"}], spec_files=FILES, artifacts=str(tmp_path))
    assert list(tmp_path.iterdir()) == []
    with pytest.raises(SpecViolation) as info:
        with check_spec(spec_files=FILES, artifacts=str(tmp_path)) as rec:
            subprocess.run([sys.executable, "-c", CHILD.format(client="child")], env=rec.env(), check=True)
            emit("eEnter", key="L9", client="parent")
    out = info.value.output_dir
    assert os.path.dirname(out) == str(tmp_path)
    assert {"events.jsonl", "report.json", "snappea-output.txt"} <= set(os.listdir(out))
    # the child's events folder is copied, since the recording deletes it
    (child_dir,) = [d for d in os.listdir(out) if d.startswith("events-dir-")]
    assert os.listdir(os.path.join(out, child_dir))
    monkeypatch.setenv("SNAPPEA_ARTIFACTS", str(tmp_path / "env"))
    with pytest.raises(SpecViolation) as info:
        check_events([{"event": "eEnter", "client": "a"}, {"event": "eEnter", "client": "b"}], spec_files=FILES)
    assert os.path.dirname(info.value.output_dir) == str(tmp_path / "env")


CHILD = "from snappea import emit; emit('eEnter', key='L9', client='{client}')"


def test_events_dir_and_env_for_child_processes(monkeypatch):
    monkeypatch.setenv("PYTHONPATH", os.pathsep.join(sys.path))
    rec = Recorder(["MutualExclusion"], spec_files=FILES)  # not started: only the child records
    subprocess.run([sys.executable, "-c", CHILD.format(client="a")], env=rec.env(), check=True)
    subprocess.run([sys.executable, "-c", CHILD.format(client="b")], env=rec.env(), check=True)
    names = os.listdir(rec.events_dir())
    assert len(names) == 2 and all(n.startswith("python-") and n.endswith(".jsonl") for n in names)
    assert rec.env()["SNAPPEA_EVENTS_DIR"] == rec.events_dir()
    with pytest.raises(SpecViolation, match="entered while"):
        rec.check()


def test_add_events_dir_and_add_log_read_in_place(tmp_path):
    folder = tmp_path / "service"
    folder.mkdir()
    (folder / "svc-1-abc.jsonl").write_text(
        '{"key":"L1","timestamp":"2026-01-01T00:00:00.000000001Z","event":"eEnter","payload":{"client":"svc"}}\n'
    )
    log = tmp_path / "other.jsonl"
    log.write_text('{"key":"L1","timestamp":"2026-01-01T00:00:00.000000002Z","event":"eEnter","payload":{"client":"log"}}\n')
    rec = Recorder(["MutualExclusion"], spec_files=FILES)
    rec.add_events_dir(str(folder))
    rec.add_log(str(log))
    with pytest.raises(SpecViolation, match="log entered while svc is inside"):
        rec.check()
    rec = Recorder(["MutualExclusion"], spec_files=FILES)
    rec.add_events_dir(str(tmp_path / "missing"))
    rec._events.append(Event("eEnter", {"client": "a"}, "L1", 1))
    with pytest.raises(SetupError) as info:
        rec.check()
    assert info.value.kind == "config"


def test_truncated_child_line_is_a_parser_error():
    rec = Recorder(["MutualExclusion"], spec_files=FILES)
    with open(os.path.join(rec.events_dir(), "worker-1-abc.jsonl"), "w") as f:
        f.write('{"key":"L1","timestamp":"2026-01-01T00:00:00Z","event":"eEnter","payload":{"cli')
    with pytest.raises(SetupError, match="not valid JSON") as info:
        rec.check()
    assert info.value.kind == "parser"
