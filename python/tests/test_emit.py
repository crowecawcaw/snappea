import enum
import json
import os
import re
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone

import pytest

from snappea import Recorder, SetupError, SpecViolation, _emit, check_spec, emit, set_sink
from snappea._recorder import _parse_time


def test_emit_does_nothing_without_a_recorder(monkeypatch, tmp_path):
    monkeypatch.delenv("SNAPPEA_EVENTS_DIR", raising=False)
    emit("eAnything", x=1)
    assert list(tmp_path.iterdir()) == []


def test_emit_writes_to_the_events_dir(monkeypatch, tmp_path):
    monkeypatch.setenv("SNAPPEA_EVENTS_DIR", str(tmp_path))
    emit("eThing", key=7, x={1, 2}, at=5)
    (f,) = tmp_path.iterdir()
    assert re.fullmatch(rf"[\w.-]+-{os.getpid()}-[0-9a-f]{{6}}\.jsonl", f.name)
    d = json.loads(f.read_text())
    assert d["key"] == "7"
    assert d["timestamp"] == "1970-01-01T00:00:00.000000005Z"
    assert d["event"] == "eThing"
    assert sorted(d["payload"]["x"]) == [1, 2]
    assert d["at"] is True
    assert d["src"].endswith("test_emit.py:22")
    _emit._file.close()
    _emit._file = None


def test_enums_are_sent_by_name():
    class Color(str, enum.Enum):
        RED = "r"

    class Size(enum.IntEnum):
        BIG = 2

    @dataclass
    class Item:
        size: Size

    payload = {"color": Color.RED, "sizes": (Size.BIG,), "item": Item(Size.BIG), Color.RED: 1}
    d = json.loads(_emit.Event("e", payload, "", 0).to_json())
    assert d["payload"] == {"color": "RED", "sizes": ["BIG"], "item": {"size": "BIG"}, "RED": 1}


def test_timestamps_round_trip():
    t = datetime(2026, 10, 18, 1, 2, 3, 456789, tzinfo=timezone.utc)
    ns = _emit.to_ns(t)
    assert _emit.iso_timestamp(ns) == "2026-10-18T01:02:03.456789000Z"
    assert _parse_time(_emit.iso_timestamp(ns + 1)) == ns + 1
    assert _emit.to_ns(t.astimezone(timezone(timedelta(hours=5)))) == ns
    with pytest.raises(TypeError, match="float"):
        _emit.to_ns(1.5)


def test_clock_never_goes_back():
    a = _emit._make_event("e", None, None, None, None)
    b = _emit._make_event("e", None, None, None, None)
    assert b.time_ns >= a.time_ns and b.seq > a.seq


@pytest.fixture
def no_recording(monkeypatch):
    """Production: no recorder and no SNAPPEA_EVENTS_DIR."""
    monkeypatch.delenv("SNAPPEA_EVENTS_DIR", raising=False)
    monkeypatch.setattr(_emit, "_recorders", [])
    yield
    set_sink(None)


def test_sink_writes_json_lines_snappea_observe_reads(no_recording, tmp_path):
    from conftest import SPEC

    log = tmp_path / "logs" / "events.jsonl"
    sink = set_sink(str(log))
    emit("eEnter", key="L1", client="c1")
    emit("eEnter", key="L1", client="c2")
    assert sink.flush()
    lines = [json.loads(x) for x in log.read_text().splitlines()]
    assert [d["payload"]["client"] for d in lines] == ["c1", "c2"]
    assert lines[0]["src"].endswith("test_emit.py:82")
    rec = Recorder(["MutualExclusion"], spec_files=[SPEC])
    rec.add_log(str(log))
    with pytest.raises(SpecViolation, match="c2 entered while c1 is inside"):
        rec.check()


def test_sink_never_raises_and_counts_drops(no_recording):
    def broken(line):
        raise OSError("disk full")

    sink = set_sink(broken)
    emit("eEnter", client="c1")
    emit("eEnter", {"client": "c1"}, client="c1")  # misuse: dropped, not raised
    emit(42)  # not a string: dropped
    sink.flush()
    assert sink.dropped == 3


def test_events_dir_never_raises_and_counts_drops(no_recording, monkeypatch, tmp_path):
    # a process a recording test started: emit runs in application code, so a payload
    # that cannot be encoded or a failed write is dropped and counted, not raised
    monkeypatch.setenv("SNAPPEA_EVENTS_DIR", str(tmp_path))
    monkeypatch.setattr(_emit, "dir_dropped", 0)
    cyclic: dict = {}
    cyclic["self"] = cyclic
    emit("eEnter", client=cyclic)
    emit("eEnter", {(1, 2): "tuple key"})
    assert _emit.dir_dropped == 2
    emit("eEnter", client="c1")  # later events still get through
    (f,) = tmp_path.iterdir()
    assert [json.loads(line)["payload"] for line in f.read_text().splitlines()] == [{"client": "c1"}]
    _emit._file.close()
    _emit._file = None
    monkeypatch.setenv("SNAPPEA_EVENTS_DIR", str(f))  # a file, not a folder: writes fail
    emit("eEnter", client="c2")
    assert _emit.dir_dropped == 3


def test_unencodable_event_in_a_recording_fails_the_check():
    from conftest import SPEC

    cyclic: dict = {}
    cyclic["self"] = cyclic
    rec = Recorder(spec_files=[SPEC])
    with rec:
        emit("eEnter", client=cyclic)  # no error here, in the code under test
    with pytest.raises(SetupError, match=r"could not be encoded(.|\n)*eEnter \(.*test_emit.py:\d+\): \w+Error") as info:
        rec.check()
    assert info.value.kind == "parser"


def test_sink_to_a_callback_and_stdout(no_recording, capsys):
    got = []
    set_sink(got.append)
    emit("eClose", 7)
    _emit._sink.flush()
    assert json.loads(got[0])["payload"] == 7
    set_sink("-")
    emit("eClose", 8)
    _emit._sink.flush()
    assert json.loads(capsys.readouterr().out)["payload"] == 8


def test_recordings_take_events_from_the_sink(no_recording):
    from conftest import SPEC

    got = []
    set_sink(got.append)
    with check_spec("MutualExclusion", spec_files=[SPEC]) as rec:
        emit("eEnter", client="c1")
    _emit._sink.flush()
    assert got == [] and rec.count("eEnter") == 1


def test_sink_drops_when_the_queue_is_full(no_recording, monkeypatch):
    import threading

    release = threading.Event()
    monkeypatch.setattr(_emit.Sink, "QUEUE_SIZE", 2)
    sink = set_sink(lambda line: release.wait())
    for _ in range(5):
        emit("eEnter", client="c1")  # never blocks
    assert sink.dropped >= 2
    release.set()
    assert sink.flush()
