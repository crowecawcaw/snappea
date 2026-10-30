"""Check P specs against events emitted by your code during its tests.

Install the ``pysnappea`` distribution and import its API from ``snappea``.

Application code calls :func:`emit`. Tests are checked either by the pytest plugin
(every test, configured under ``[tool.snappea]`` in pyproject.toml) or explicitly with
:func:`check_spec`, :func:`expect_violation` and :func:`check_events`. In production,
:func:`set_sink` sends the same events to a file, stdout or a callback for
``snappea observe``.
"""

from __future__ import annotations

import re
from typing import Any, Optional, Sequence

from ._binary import find_snappea
from ._emit import Event, Sink, emit, set_sink
from ._errors import SetupError, StrictDefaultWarning
from ._recorder import Recorder, SpecViolation, _deprecated, check_hot_arg, events_from_dicts

# show `snappea.SpecViolation` in tracebacks, not the private module
for _cls in (Recorder, Event, SpecViolation, SetupError, StrictDefaultWarning, Sink):
    _cls.__module__ = "snappea"

__all__ = [
    "emit",
    "set_sink",
    "Sink",
    "check_spec",
    "expect_violation",
    "check_events",
    "Recorder",
    "Event",
    "SpecViolation",
    "SetupError",
    "StrictDefaultWarning",
    "find_snappea",
]


def __getattr__(name: str) -> Any:
    if name == "SnappeaError":
        _deprecated("snappea.SnappeaError", "snappea.SetupError")
        return SetupError
    raise AttributeError(f"module 'snappea' has no attribute {name!r}")


class check_spec:
    """Checks the events emitted inside the block against P specs::

        with check_spec("MutualExclusion") as rec:
            server.acquire("c1")
            server.acquire("c2")

    The block observes events sent with :func:`emit`; it does not intercept ordinary
    method calls or run P drivers.
    No spec names means every spec in the spec files. Raises :class:`SpecViolation`
    (an ``AssertionError``) at the end of the block, so the test fails on this line,
    or :class:`SetupError` if the check cannot run. Options are those of
    :class:`Recorder`: ``check_hot=True`` also fails if a spec is left in a hot state;
    ``strict`` (default: true when specs are named) requires relevant events for every
    selected spec. When this named-spec default applies, missing relevant events currently
    warn; set ``strict=True`` explicitly to fail on them. ``unknown_events`` ("error" by
    default) handles undeclared names.
    """

    def __init__(
        self,
        *specs: str,
        spec_files: Optional[Sequence[str]] = None,
        check_hot: Optional[bool] = None,
        strict: Optional[bool] = None,
        unknown_events: Optional[str] = None,
        timeout: Optional[float] = None,
        artifacts: Optional[str] = None,
        liveness: Optional[bool] = None,
    ):
        self.recorder = Recorder(
            specs,
            spec_files=spec_files,
            check_hot=check_hot_arg(check_hot, liveness),
            strict=strict,
            unknown_events=unknown_events,
            timeout=timeout,
            artifacts=artifacts,
        )

    def __enter__(self) -> Recorder:
        return self.recorder.start()

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        __tracebackhide__ = True
        self.recorder.stop()
        if exc_type is None:
            self.recorder.check()
        else:
            self.recorder._cleanup_owned()


class expect_violation:
    """The block must break `spec`, like ``pytest.raises``::

        with expect_violation("MutualExclusion", match="c2 entered"):
            server.acquire("c1")
            server.acquire("c2")

    The block passes if a :class:`SpecViolation` of `spec` is raised inside it (by
    :func:`check_events` or a nested :func:`check_spec`) or if the events emitted inside
    it break the spec. ``match`` is a regular expression searched for in the spec's
    message. Enclosing checks, such as the pytest plugin's, ignore `spec` for the rest of
    the test. The violation is available as ``.violation`` afterwards.
    """

    def __init__(
        self,
        spec: str,
        match: Optional[str] = None,
        *,
        spec_files: Optional[Sequence[str]] = None,
        check_hot: Optional[bool] = None,
        unknown_events: Optional[str] = None,
        timeout: Optional[float] = None,
        artifacts: Optional[str] = None,
        liveness: Optional[bool] = None,
    ):
        self.spec = spec
        self.match = match
        self.recorder = Recorder(
            [spec],
            spec_files=spec_files,
            check_hot=check_hot_arg(check_hot, liveness),
            strict=False,  # the spec must break; whether it saw events is beside the point
            unknown_events=unknown_events,
            timeout=timeout,
            artifacts=artifacts,
        )
        self.violation: Optional[SpecViolation] = None

    def __enter__(self) -> "expect_violation":
        from . import _emit

        for outer in _emit._recorders:
            outer.ignored.add(self.spec)
        self.recorder.start()
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        __tracebackhide__ = True
        self.recorder.stop()
        found: Optional[SpecViolation] = None
        if exc_type is not None:
            if isinstance(exc, SpecViolation):
                found = next((v for v in [exc, *exc.others] if v.spec == self.spec), None)
            if found is None:
                self.recorder._cleanup_owned()
                return False  # some other failure: let it through
        else:
            found = next((v for v in self.recorder.violations() if v.spec == self.spec), None)
        if found is None:
            n = len(self.recorder.events)
            raise AssertionError(f"expected P spec {self.spec} to be broken, but it held over {n} event(s)")
        if self.match is not None and not re.search(self.match, found.message):
            raise AssertionError(
                f"P spec {self.spec} was broken, but its message {found.message!r} "
                f"does not match {self.match!r}"
            )
        self.violation = found
        return True


def check_events(
    events: Sequence[Any],
    *specs: str,
    spec_files: Optional[Sequence[str]] = None,
    check_hot: Optional[bool] = None,
    strict: Optional[bool] = None,
    unknown_events: Optional[str] = None,
    timeout: Optional[float] = None,
    artifacts: Optional[str] = None,
    liveness: Optional[bool] = None,
) -> None:
    """Check captured implementation events against P specs::

        check_events(captured_events, "MutualExclusion")

    Events should normally come from observed implementation behavior, with payloads
    describing actual results and state. In Python tests, prefer application calls
    to :func:`emit`, captured automatically by the pytest plugin or inside
    :func:`check_spec`::

        with check_spec("MutualExclusion"):
            server.acquire("c1")
            server.acquire("c2")

    Use this function when you already have a captured event list. Hand-written
    traces are useful for testing the spec itself; they do not validate the
    implementation. For event logs, use ``snappea observe``.

    Events take ``event`` and ``payload`` (or payload fields directly, as with
    :func:`emit`) and optionally ``key`` and ``at``; without ``at`` they are checked in
    list order. This checks the supplied trace against P monitors; it does not run P test
    cases or explore schedules. No P entry machine or test declaration is needed for
    observation. Keep observation-only monitors outside a model test project, or use a
    reasoned ``unselected-spec`` lint suppression for intentional observation-only use.
    When adapting captured data, preserve the identities and values the property needs;
    collapsing distinct results can hide violations. These events do not reach any
    enclosing check. Options are those of :func:`check_spec`.
    """
    __tracebackhide__ = True
    rec = Recorder(
        specs,
        spec_files=spec_files,
        check_hot=check_hot_arg(check_hot, liveness),
        strict=strict,
        unknown_events=unknown_events,
        timeout=timeout,
        artifacts=artifacts,
    )
    rec._events.extend(events_from_dicts(events))
    rec.check()
