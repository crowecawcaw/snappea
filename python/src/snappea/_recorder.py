"""The recorder both modes share: collects events, runs `snappea observe`, builds reports."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from typing import Any, Callable, Dict, Iterable, List, Optional, Sequence, Tuple

from . import _config, _emit
from ._binary import find_snappea
from ._emit import Event
from ._errors import SetupError, warn

#: Events shown in a failure report, counting back from the one that broke the spec.
REPORT_EVENTS = 20

LogParser = Callable[[str], Optional[dict]]


def _deprecated(old: str, new: str) -> None:
    warn(f"{old} is deprecated; use {new}", DeprecationWarning)


def check_hot_arg(check_hot: Optional[bool], liveness: Optional[bool]) -> Optional[bool]:
    """`check_hot`, or the deprecated `liveness=` spelling of it."""
    if liveness is None:
        return check_hot
    if check_hot is not None:
        raise TypeError("pass check_hot=, not both check_hot= and liveness=")
    _deprecated("liveness=", "check_hot=")
    return liveness


class SpecViolation(AssertionError):
    """A P spec was broken by the recorded events."""

    def __init__(
        self,
        spec: str,
        message: str,
        key: str = "",
        events: Sequence[Event] = (),
        *,
        location: Optional[str] = None,
        liveness: bool = False,
        earlier: int = 0,
        output_dir: Optional[str] = None,
        others: Sequence["SpecViolation"] = (),
    ):
        #: Name of the broken spec.
        self.spec = spec
        #: The spec's assert message (or, for liveness, which hot state it ended in).
        self.message = message
        #: Event key (stream) the violation happened on.
        self.key = key
        #: Events of that key up to and including the violating one (the last ones only).
        self.events = list(events)
        #: `file:line` of the failed assert in the P sources.
        self.location = location
        #: The spec ended in a hot state (found with `check_hot`).
        self.liveness = liveness
        #: Events of this key before `events` that are not shown.
        self.earlier = earlier
        #: Folder with the events and snappea's full output (the check's artifacts).
        self.output_dir = output_dir
        #: Further violations found by the same check.
        self.others = list(others)
        super().__init__(self.report())

    def report(self) -> str:
        lines = []
        if self.liveness:
            lines.append(f"P spec {self.spec} was still waiting when the events ended: {self.message}")
        else:
            lines.append(f"P spec {self.spec} was broken: {self.message}")
        lines.append("")
        lines.append(f'  Events (key "{self.key}"):' if self.key else "  Events:")
        if self.earlier:
            lines.append(f"     ... {self.earlier} earlier event(s)")
        rows = [(_payload_text(e.payload), e) for e in self.events]
        w_event = max([len(e.event) for _, e in rows] + [0])
        w_payload = min(max([len(p) for p, _ in rows] + [0]), 48)
        for i, (p, e) in enumerate(rows):
            n = self.earlier + i + 1
            row = f"    {n:>3}  {e.event:<{w_event}}  {p:<{w_payload}}  {e.src or ''}"
            if e.at:
                row += "  (time set with at=)"
            if i == len(rows) - 1:
                row += "  <- events ended here" if self.liveness else "  <- broke the spec here"
            lines.append(row.rstrip() if i < len(rows) - 1 else row)
        if not rows:
            lines.append("    (none)")
        lines.append("")
        if self.location:
            lines.append(f"  Spec: {self.location}")
        if self.output_dir:
            lines.append(f"  snappea output: {self.output_dir}")
        if self.others:
            lines.append("")
            lines.append(f"  {len(self.others)} more violation(s):")
            for o in self.others:
                key = f' (key "{o.key}")' if o.key else ""
                lines.append(f"    {o.spec}{key}: {o.message}")
        return "\n".join(lines)

    def __str__(self) -> str:
        return self.report()


@dataclass
class _Settings:
    """A recorder's options after applying `[tool.snappea]`, the environment and defaults."""

    specs: List[str]
    exclude: List[str]
    strict: bool
    check_hot: bool
    unknown_events: str
    timeout: float
    artifacts: Optional[str]
    config: _config.Config


class Recorder:
    """Collects the events emitted while it is active and checks them against P specs.

    Every option left as ``None`` comes from ``[tool.snappea]`` in the nearest
    pyproject.toml (and the environment variables that override it), else its default:

    * ``specs``: the specs to check; none means every spec in the spec files
      (except ``exclude``).
    * ``spec_files``: the P files: paths, globs, or folders meaning every ``.p`` in
      them. Relative to the working directory here; to the pyproject.toml in config.
    * ``strict``: require a relevant event for every selected spec. Default: true when
      ``specs`` are named, false for every spec. When this named-spec default applies,
      missing relevant events currently warn; set ``strict=True`` explicitly to fail.
    * ``check_hot``: also fail on a spec left in a hot state. Default: false.
    * ``unknown_events``: an event name no P file declares is an ``"error"`` (default),
      a ``"warn"``\\ ing, or ``"ignore"``\\ d.
    * ``timeout``: seconds the checker may run. Default: 30.
    * ``artifacts``: folder to keep a failed check's events and output in. Default: a
      temp folder. ``SNAPPEA_ARTIFACTS`` overrides ``[tool.snappea]``.

    After a check, :attr:`report` holds snappea's JSON result, with each spec's
    monitor coverage.
    """

    def __init__(
        self,
        specs: Iterable[str] = (),
        *,
        spec_files: Optional[Sequence[str]] = None,
        exclude: Optional[Iterable[str]] = None,
        strict: Optional[bool] = None,
        check_hot: Optional[bool] = None,
        unknown_events: Optional[str] = None,
        timeout: Optional[float] = None,
        artifacts: Optional[str] = None,
        liveness: Optional[bool] = None,
        output_root: Optional[str] = None,
    ):
        if isinstance(specs, str):
            specs = [specs]
        self.specs: List[str] = list(specs)
        self.spec_files = [os.fspath(f) for f in spec_files] if spec_files is not None else None
        self.exclude: Optional[List[str]] = list(exclude) if exclude is not None else None
        self.check_hot = check_hot_arg(check_hot, liveness)
        self.strict = strict
        if unknown_events is not None:
            _config.check_unknown_events(unknown_events, "unknown_events")
        self.unknown_events = unknown_events
        self.timeout = timeout
        if output_root is not None:
            _deprecated("output_root=", "artifacts=")
            artifacts = artifacts or output_root
        self.artifacts = os.fspath(artifacts) if artifacts is not None else None
        #: snappea's JSON result of the last check that ran (``specs[].coverage`` holds
        #: each spec's monitor coverage); None before, or if snappea wrote none.
        self.report: Optional[dict] = None
        #: Specs whose violations this recorder ignores (broken on purpose inside
        #: `expect_violation`).
        self.ignored: set = set()
        self._events: List[Event] = []
        self._logs: List[Tuple[str, Optional[LogParser]]] = []
        self._dirs: List[str] = []
        self._owned: List[str] = []
        self._active = False
        self._prev_env: Optional[str] = None
        self._child_dir: Optional[str] = None
        # set by the pytest plugin: its configuration, and pytest's temp folder
        self._cfg: Optional[_config.Config] = None
        self._default_artifacts: Optional[str] = None

    @property
    def liveness(self) -> Optional[bool]:
        """Deprecated: use :attr:`check_hot`."""
        _deprecated("Recorder.liveness", "Recorder.check_hot")
        return self.check_hot

    # ---- collecting ----------------------------------------------------------------

    def start(self) -> "Recorder":
        """Starts collecting: events emitted in this process, and in processes started
        from now on (through the `SNAPPEA_EVENTS_DIR` environment variable)."""
        if self._active:
            return self
        events_dir = self.events_dir()
        self._prev_env = os.environ.get(_emit.EVENTS_DIR_ENV)
        os.environ[_emit.EVENTS_DIR_ENV] = events_dir
        _emit._recorders.append(self)
        self._active = True
        return self

    def stop(self) -> None:
        """Stops collecting. Events child processes wrote are passed on to the recorders
        still active, so an enclosing test-wide check sees them too."""
        if not self._active:
            return
        self._active = False
        if self in _emit._recorders:
            _emit._recorders.remove(self)
        if self._prev_env is None:
            os.environ.pop(_emit.EVENTS_DIR_ENV, None)
        else:
            os.environ[_emit.EVENTS_DIR_ENV] = self._prev_env
        if _emit._recorders and self._child_dir and self._child_dir in self._owned:
            for outer in _emit._recorders:
                outer._dirs.append(self._child_dir)
            # the outermost recorder deletes the folder when it is done
            _emit._recorders[0]._owned.append(self._child_dir)
            self._owned.remove(self._child_dir)

    def __enter__(self) -> "Recorder":
        return self.start()

    def __exit__(self, *exc: Any) -> None:
        self.stop()

    def _add(self, ev: Event) -> None:
        self._events.append(ev)

    def events_dir(self) -> str:
        """This recording's events folder: the `SNAPPEA_EVENTS_DIR` of processes it starts.
        Created on first use; deleted after the check."""
        if self._child_dir is None:
            self._child_dir = tempfile.mkdtemp(prefix="snappea-events-")
            self._owned.append(self._child_dir)
            self._dirs.append(self._child_dir)
        return self._child_dir

    def env(self) -> Dict[str, str]:
        """The current environment plus `SNAPPEA_EVENTS_DIR`, for starting a child process:
        ``subprocess.run(cmd, env=rec.env())``."""
        return {**os.environ, _emit.EVENTS_DIR_ENV: self.events_dir()}

    def add_log(self, path: str, parse: Optional[LogParser] = None) -> None:
        """Also check the events in the log file at `path`, read when the check runs.

        ``parse`` turns one line into an event or ``None``: a dict like ``emit``'s
        arguments, ``{"event": "eEnter", "key": "L1", "at": when, "client": "c1"}``
        (``at`` is required so events can be merged by time). Without ``parse`` the file
        must hold snappea's JSON lines."""
        self._logs.append((os.path.abspath(os.fspath(path)), parse))

    def add_events_dir(self, path: str) -> None:
        """Also check the JSON-lines files in folder `path`, e.g. one mounted into a
        container whose service writes events there. The folder must exist at check time."""
        self._dirs.append(os.path.abspath(os.fspath(path)))

    # ---- looking at events -----------------------------------------------------------

    @property
    def events(self) -> List[Event]:
        """Every event collected so far: emitted here, by child processes and in logs,
        in time order."""
        evs = list(self._events) + [e for _, e in self._external_lines()]
        evs.sort(key=lambda e: e.time_ns)
        return evs

    def events_named(self, name: str) -> List[Any]:
        """The payloads of the events called `name`, in order."""
        return [e.payload for e in self.events if e.event == name]

    def count(self, name: str) -> int:
        return sum(1 for e in self.events if e.event == name)

    def _dir_files(self) -> List[str]:
        out = []
        for d in self._dirs:
            if os.path.isdir(d):
                out.extend(p for p in (os.path.join(d, n) for n in sorted(os.listdir(d))) if os.path.isfile(p))
        return out

    def _external_lines(self) -> List[Tuple[str, Event]]:
        """(JSON line, event) for events written by other processes and in logs."""
        out: List[Tuple[str, Event]] = []
        for p in self._dir_files():
            with open(p, encoding="utf-8", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        out.append((line, _event_of_line(line)))
        for path, parse in self._logs:
            with open(path, encoding="utf-8", errors="replace") as f:
                if parse is None:
                    out.extend((line, _event_of_line(line)) for line in (x.strip() for x in f) if line)
                else:
                    out.extend((ev.to_json(), ev) for ev in _parse_log(f, path, parse))
        return out

    # ---- checking --------------------------------------------------------------------

    def check(self) -> None:
        """Runs snappea over the events; raises SpecViolation if a spec was broken, and
        SetupError if the check could not run."""
        __tracebackhide__ = True
        violations = self.violations()
        if violations:
            first = violations[0]
            first.others = violations[1:]
            first.args = (first.report(),)
            raise first

    def violations(self) -> List[SpecViolation]:
        """Runs snappea over the events and returns the violations found (empty if none)."""
        __tracebackhide__ = True
        try:
            settings = self._settings()
            if not self._events and not self._logs and not any(os.path.getsize(p) for p in self._dir_files()):
                # nothing happened: every spec still holds, unless strict wants events
                if not settings.strict:
                    return []
            return self._run(settings)
        finally:
            self._cleanup_owned()

    def _cleanup_owned(self) -> None:
        if self._active:
            return
        for d in self._owned:
            shutil.rmtree(d, ignore_errors=True)
        self._owned.clear()

    def _settings(self) -> _Settings:
        cfg = self._cfg if self._cfg is not None else _config.load(os.getcwd())
        specs = self.specs or list(cfg.specs)
        if self.strict is not None:
            strict = self.strict
        elif cfg.strict is not None:
            strict = cfg.strict
        else:
            # named specs default to strict: a named spec that saw no events is likely a
            # mistake (missing instrumentation, wrong key, wrong spec name)
            strict = bool(specs)
        unknown = self.unknown_events or cfg.unknown_events or "error"
        timeout = self.timeout if self.timeout is not None else cfg.timeout or _config.DEFAULT_TIMEOUT
        if self.artifacts is not None:
            artifacts: Optional[str] = os.path.abspath(self.artifacts)
        else:
            artifacts = cfg.artifacts or self._default_artifacts
        return _Settings(
            specs=specs,
            exclude=self.exclude if self.exclude is not None else list(cfg.exclude),
            strict=strict,
            check_hot=bool(self.check_hot if self.check_hot is not None else cfg.check_hot),
            unknown_events=unknown,
            timeout=timeout,
            artifacts=artifacts,
            config=cfg,
        )

    def _files(self, cfg: _config.Config) -> List[str]:
        if self.spec_files is not None:
            # globs and folders, relative to the working directory
            cfg = _config.Config(spec_files=self.spec_files, root=os.getcwd())
        files = [os.path.abspath(f) for f in cfg.files()]
        if not files:
            raise SetupError(
                "No P spec files to check: set spec_files under [tool.snappea] in pyproject.toml, "
                "set SNAPPEA_SPEC_FILES, or pass spec_files=."
            )
        return files

    def _run(self, s: _Settings) -> List[SpecViolation]:
        __tracebackhide__ = True
        files = self._files(s.config)
        own_lines = _encode(self._events)
        if s.artifacts:
            os.makedirs(s.artifacts, exist_ok=True)
        workdir = os.path.abspath(tempfile.mkdtemp(prefix="snappea-check-", dir=s.artifacts))
        # snappea reports P source paths relative to its working folder: the project's
        # root, so coverage of every check names the same files and merges
        cwd = os.path.abspath(s.config.root)
        # this process's events first: equal timestamps keep input order
        own = os.path.join(workdir, "events.jsonl")
        with open(own, "w", encoding="utf-8") as f:
            f.writelines(own_lines)
        inputs = [own, *self._dirs]
        for i, (path, parse) in enumerate(self._logs):
            if parse is None:
                inputs.append(path)
                continue
            parsed = os.path.join(workdir, f"log-{i}-{os.path.basename(path)}.jsonl")
            with open(path, encoding="utf-8", errors="replace") as src, open(parsed, "w", encoding="utf-8") as dst:
                dst.writelines(_encode(_parse_log(src, path, parse)))
            inputs.append(parsed)
        report_path = os.path.join(workdir, "report.json")
        cmd = [find_snappea(), "observe", *files]
        # logs as -l options, so a log name never reads as a P source
        for path in inputs:
            cmd += ["-l", path]
        if s.specs:
            cmd += ["--spec", ",".join(s.specs)]
        else:
            cmd += ["--all-specs"]
            if s.exclude:
                cmd += ["--exclude-specs", ",".join(s.exclude)]
        cmd += ["--json-report", report_path, "--unknown-events", s.unknown_events]
        if s.check_hot:
            cmd.append("--check-hot")
        if s.strict:
            cmd.append("--strict")
        # the output is kept for people to read; the data comes from the JSON report
        cmd += ["--format", "text"]
        try:
            proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, cwd=cwd, timeout=s.timeout)
        except subprocess.TimeoutExpired:
            self._keep_children(workdir)
            raise SetupError(
                f"snappea observe did not finish within {s.timeout:g} s; raise timeout= (or "
                f"[tool.snappea] timeout) if the events are many.\n\nsnappea output: {workdir}",
                "timeout",
                output_dir=workdir,
            ) from None
        output = proc.stdout.decode("utf-8", errors="replace")
        with open(os.path.join(workdir, "snappea-output.txt"), "w", encoding="utf-8") as f:
            f.write(" ".join(cmd) + "\n\n" + output)
        report = None
        if os.path.isfile(report_path):
            with open(report_path, encoding="utf-8") as f:
                report = json.load(f)
            self.report = report
            for w in _unique(w for spec in report.get("specs", []) for w in spec.get("warnings", [])):
                warn(_with_src(w, report, cwd), UserWarning)
        if proc.returncode == 0:
            shutil.rmtree(workdir, ignore_errors=True)
            return []
        self._keep_children(workdir)
        if report is None or "specs" not in report:
            # snappea stopped before checking (bad options or P sources)
            raise _command_error(output or (report or {}).get("error", ""), workdir)
        return self._violations_of(report, workdir, cwd, s)

    def _keep_children(self, workdir: str) -> None:
        """Copies the events folders this process created into the artifacts, since
        they are deleted after the check."""
        for i, d in enumerate(self._dirs):
            if os.path.basename(d).startswith("snappea-events-") and os.path.isdir(d):
                shutil.copytree(d, os.path.join(workdir, f"events-dir-{i}"), dirs_exist_ok=True)

    def _violations_of(self, report: dict, workdir: str, cwd: str, s: _Settings) -> List[SpecViolation]:
        __tracebackhide__ = True
        violations: List[SpecViolation] = []
        bad_lines: Dict[str, str] = {}
        unknown: Dict[str, dict] = {}
        validation: List[str] = []
        other: List[Tuple[str, str]] = []
        for spec_report in report.get("specs", []):
            spec = spec_report["spec"]
            for err in spec_report.get("errors", []):
                kind = err.get("kind")
                if kind == "parser":
                    bad_lines.setdefault(err.get("message", ""), err.get("cause") or "")
                elif kind == "unknown_event":
                    for u in spec_report.get("unknown_events", []):
                        unknown.setdefault(u.get("event", ""), u)
                    if not spec_report.get("unknown_events"):
                        other.append((kind, err.get("message", "")))
                elif kind == "validation":
                    if spec not in self.ignored:
                        validation.append(err.get("message", "Strict observation failed"))
                elif kind == "spec_violation" or (kind == "unknown" and err.get("events")):
                    if spec not in self.ignored:
                        violations.append(_violation(spec, err, workdir, cwd))
                else:
                    other.append((kind or "unknown", err.get("message") or kind or "error"))
        if violations:
            return violations
        if bad_lines:
            raise SetupError(_payload_error(bad_lines, workdir), "parser", output_dir=workdir)
        if unknown:
            raise SetupError(_unknown_error(unknown, workdir, cwd), "unknown_event", output_dir=workdir)
        if other:
            kind = other[0][0]
            text = "\n".join(_unique(m for _, m in other))
            raise SetupError(f"snappea could not check the events ({kind}):\n{text}\n\nsnappea output: {workdir}", kind, output_dir=workdir)
        if validation:
            raise SetupError(_validation_error(validation, workdir), "validation", output_dir=workdir)
        shutil.rmtree(workdir, ignore_errors=True)
        return []


def _validation_error(messages: List[str], workdir: str) -> str:
    return (
        "\n".join(_unique(messages))
        + "\n\nEmit the events these specs watch in the code under test (check the event names "
        "and keys), or pass strict=False (or set [tool.snappea] strict = false) if a spec may "
        "rightly see no events here."
        + f"\nsnappea output: {workdir}"
    )


def _encode(events: Iterable[Event]) -> List[str]:
    """The JSON lines of `events`. An event that cannot be encoded fails the check:
    dropping it would check incomplete evidence."""
    lines: List[str] = []
    bad: List[Tuple[Event, Exception]] = []
    for ev in events:
        try:
            lines.append(ev.to_json() + "\n")
        except Exception as e:  # circular payload, non-string dict keys, a failing __str__
            bad.append((ev, e))
    if not bad:
        return lines
    out = ["Some events could not be encoded as JSON, so the check would miss them:"]
    for ev, e in bad[:10]:
        where = f" ({ev.src})" if ev.src else ""
        out.append(f"  {ev.event}{where}: {type(e).__name__}: {e}")
    if len(bad) > 10:
        out.append(f"  ... and {len(bad) - 10} more")
    out.append("")
    out.append(
        "Pass payload values JSON can hold: str, int, float, bool, None, lists, dicts with "
        "string keys, enums and dataclasses of these, with no cycles."
    )
    raise SetupError("\n".join(out), "parser")


def _unique(items: Iterable[str]) -> List[str]:
    out: List[str] = []
    for x in items:
        if x not in out:
            out.append(x)
    return out


def _parse_log(lines: Iterable[str], path: str, parse: LogParser) -> List[Event]:
    out = []
    for lineno, line in enumerate(lines, 1):
        d = parse(line.rstrip("\n"))
        if d is not None:
            out.append(_event_from_dict(d, f"{path}:{lineno}", what=f"the parser of {path}"))
    return out


def _violation(spec: str, err: dict, workdir: str, cwd: str) -> SpecViolation:
    message = err.get("message") or ""
    location = None
    liveness = False
    m = re.match(r"Assertion failure: (.+?:\d+):\d+ (.*)$", message, re.S)
    if m:
        location, message = _rel(m.group(1), cwd), m.group(2)
    else:
        m = re.match(r".*detected liveness bug in hot state '(.*)' at the end of program execution\.$", message)
        if m:
            liveness = True
            message = f"it ended in hot state '{m.group(1)}'"
        elif message in ("", "null"):
            message = err.get("kind", "error")
    events = [_event_of_line(e.get("log_line") or "{}") for e in err.get("events", [])]
    for ev, raw in zip(events, err.get("events", [])):
        ev.event = ev.event or raw.get("event", "")
    earlier = max(0, len(events) - REPORT_EVENTS)
    return SpecViolation(
        spec,
        message,
        err.get("key") or "",
        events[earlier:],
        location=location,
        liveness=liveness,
        earlier=earlier,
        output_dir=workdir,
    )


def _event_of_line(line: str) -> Event:
    try:
        d = json.loads(line)
    except ValueError:
        return Event(event="?", payload=line, key="", time_ns=0)
    if not isinstance(d, dict):
        return Event(event="?", payload=d, key="", time_ns=0)
    ev = Event.from_json(d)
    ts = d.get("timestamp")
    ev.time_ns = _parse_time(ts)
    return ev


def _parse_time(ts: Any) -> int:
    if isinstance(ts, int):
        return ts * 1_000_000  # snappea reads integers as milliseconds
    if isinstance(ts, str):
        if ts.isdigit():
            return int(ts) * 1_000_000
        m = re.match(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.(\d+))?Z$", ts)
        if m:
            import calendar
            import time as _time

            secs = calendar.timegm(_time.strptime(m.group(1), "%Y-%m-%dT%H:%M:%S"))
            frac = (m.group(2) or "").ljust(9, "0")[:9]
            return secs * 1_000_000_000 + int(frac)
        try:
            from datetime import datetime

            return _emit.to_ns(datetime.fromisoformat(ts.replace("Z", "+00:00")))
        except ValueError:
            return 0
    return 0


def _event_from_dict(d: dict, src: str, what: str) -> Event:
    if not isinstance(d, dict) or not isinstance(d.get("event"), str):
        raise SetupError(f"{what} must return a dict with an 'event' name, or None; got {d!r}")
    d = dict(d)
    name = d.pop("event")
    key = d.pop("key", None)
    src = d.pop("src", None) or src
    if "at" in d:
        t = _emit.to_ns(d.pop("at"))
    elif "timestamp" in d:
        t = _parse_time(d.pop("timestamp"))
    else:
        raise SetupError(f"{what} must give each event a time ('at'), so events can be merged by time; got {d!r}")
    if "payload" in d:
        payload = d.pop("payload")
        if d:
            raise SetupError(f"{what}: give either 'payload' or payload fields, not both; got {d!r}")
    else:
        payload = d or None
    return Event(name, payload, _emit._key_str(key), t, False, src)


def events_from_dicts(events: Sequence[Any]) -> List[Event]:
    """Hand-written events for `check_events`: dicts like ``{"event": "eEnter",
    "payload": {"client": "c1"}}`` (``key`` and ``at`` optional) or ``Event`` objects.
    Events without a time are ordered as listed."""
    out: List[Event] = []
    for i, d in enumerate(events, 1):
        if isinstance(d, Event):
            out.append(d)
            continue
        d = dict(d)
        if "at" not in d and "timestamp" not in d:
            d["at"] = i
        d.setdefault("src", f"events[{i - 1}]")
        ev = _event_from_dict(d, f"events[{i - 1}]", what="check_events")
        ev.seq = i
        out.append(ev)
    return out


def _payload_text(p: Any) -> str:
    if p is None:
        return ""
    if isinstance(p, dict):
        return " ".join(f"{k}={_short(v)}" for k, v in p.items())
    return _short(p)


def _short(v: Any) -> str:
    s = v if isinstance(v, str) else json.dumps(v, default=str)
    return s if len(s) <= 40 else s[:37] + "..."


def _rel(path_loc: str, base: str) -> str:
    """`path:line` from snappea (relative to `base`, where it ran), relative to the cwd."""
    path, _, line = path_loc.rpartition(":")
    path = os.path.normpath(os.path.join(base, path))
    try:
        rel = os.path.relpath(path)
        if not rel.startswith(".."):
            path = rel
    except ValueError:
        pass
    return f"{path}:{line}"


def _command_error(output: str, workdir: str) -> SetupError:
    text = output.strip()
    outdated = None
    if text.startswith("Failed parsing"):
        # a snappea from before `snappea observe` had its own options (PObserve style)
        outdated = "uses the old PObserve-style `snappea observe` options"
    else:
        m = re.search(r"^error: unknown option '(--[\w-]+)'", text, re.M)
        if m:
            outdated = f"does not support {m.group(1)}"
    if outdated:
        return SetupError(
            f"{find_snappea()} ({_binary_version()}) {outdated}, so it is too old for this snappea "
            f"package ({_package_version()}). Install a matching snappea or unset SNAPPEA_BIN.\n\n"
            f"snappea output: {workdir}",
            "config",
            output_dir=workdir,
        )
    text = "\n".join(line for line in text.splitlines() if not line.startswith("Run `snappea observe --help`"))
    return SetupError(f"snappea could not check the events:\n{text}\n\nsnappea output: {workdir}", "config", output_dir=workdir)


def _binary_version() -> str:
    try:
        out = subprocess.run([find_snappea(), "--version"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=10)
        return out.stdout.decode("utf-8", errors="replace").strip() or "unknown version"
    except (OSError, subprocess.SubprocessError):
        return "unknown version"


def _package_version() -> str:
    try:
        from importlib.metadata import version

        return f"snappea {version('pysnappea')}"
    except Exception:
        return "snappea, unknown version"


def _src_at(location: str, cwd: str) -> Optional[str]:
    """The `src` of the event line at `path:line` (relative to `cwd`, where snappea
    ran), if it has one."""
    path, _, line = location.rpartition(":")
    path = os.path.join(cwd, path)
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for i, text in enumerate(f, 1):
                if i == int(line):
                    src = json.loads(text).get("src")
                    return src if isinstance(src, str) else None
    except (OSError, ValueError, AttributeError):
        pass
    return None


def _with_src(message: str, report: dict, cwd: str) -> str:
    """An unknown-event warning, with the emit location instead of the events file."""
    for spec in report.get("specs", []):
        for u in spec.get("unknown_events", []):
            loc = u.get("location") or ""
            src = _src_at(loc, cwd) if loc else None
            if src and loc in message:
                return message.replace(loc, src)
    return message


def _unknown_error(unknown: Dict[str, dict], workdir: str, cwd: str) -> str:
    lines = ["Some events are declared in no P spec file (a typo, or a missing spec file?):"]
    for name, u in unknown.items():
        loc = u.get("location") or ""
        where = (_src_at(loc, cwd) if loc else None) or loc
        n = u.get("count", 1)
        line = f"  {name}: {n} time(s), first at {where}"
        if u.get("suggestion"):
            line += f"; did you mean {u['suggestion']}?"
        lines.append(line)
    lines.append("")
    lines.append("Pass unknown_events='warn' or 'ignore' (or set [tool.snappea] unknown_events) to allow them.")
    lines.append(f"snappea output: {workdir}")
    return "\n".join(lines)


def _payload_error(bad: Dict[str, str], workdir: str) -> str:
    lines: List[str] = []
    mismatched, broken = [], []
    for line, cause in bad.items():
        try:
            json.loads(line)
            mismatched.append((line, cause))
        except ValueError:
            broken.append((line, cause))
    if broken:
        lines.append("Some lines are not valid JSON (cut short by a crashed or unfinished writer?):")
        for line, cause in broken[:10]:
            lines.append(f"  {line[:200]}")
            lines.append(f"    {cause}")
    if mismatched:
        lines.append("Some events do not match their declaration in the P spec files:")
        for line, cause in mismatched[:10]:
            ev = _event_of_line(line)
            where = f" ({ev.src})" if ev.src else ""
            lines.append(f"  {ev.event}{where}: {cause or 'payload does not match'}")
            lines.append(f"    payload: {json.dumps(ev.payload, default=str)}")
    if len(bad) > 10:
        lines.append(f"  ... and {len(bad) - 10} more")
    lines.append("")
    lines.append(f"snappea output: {workdir}")
    return "\n".join(lines)
