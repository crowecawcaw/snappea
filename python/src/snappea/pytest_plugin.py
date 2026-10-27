"""Global mode: every test also checks the P specs against the events it emitted.

Loaded automatically when the package is installed (the ``pytest11`` entry point).
Configured under ``[tool.snappea]`` in pyproject.toml::

    [tool.snappea]
    spec_files = ["specs/*.p"]
    exclude = ["DraftSpec"]   # optional: specs not checked anywhere yet
    mode = "auto"             # optional: "auto" (every test) | "marked" | "off"

Per test: ``@pytest.mark.snappea(skip=True)``, ``@pytest.mark.snappea(skip=["DraftSpec"])``,
``@pytest.mark.snappea(check_hot=True)``, ``@pytest.mark.snappea(strict=True)``.

Plugin checks are not strict unless asked (``strict = true``, ``--snappea-strict`` or the
mark): most tests in a suite emit no events for a given spec. Explicit checks
(``Recorder``, ``check_spec``) with named specs are strict by default. The ``snappea_events`` fixture gives the test's recorder.
"""

from __future__ import annotations

import contextlib
import json
import os
import subprocess
from pathlib import Path
from typing import Any, Generator, Iterator, List, Optional

import pytest

from . import _config
from ._binary import find_snappea
from ._errors import SetupError
from ._recorder import Recorder

_RECORDER = pytest.StashKey[Optional[Recorder]]()
_CONFIG = pytest.StashKey[Optional[_config.Config]]()
_ERROR = pytest.StashKey[Optional[str]]()
_COVERAGE = pytest.StashKey[list]()
_COVERAGE_PATH = pytest.StashKey[Optional[Path]]()
_COVERAGE_SUMMARY = pytest.StashKey[tuple]()
#: Report suite coverage at the end of the run without a file (no path set, not turned off).
_COVERAGE_DEFAULT = pytest.StashKey[bool]()
#: How xdist workers hand their reports to the controller.
_WORKER_KEY = "snappea_coverage_reports"

MARK_KEYS = {"skip", "check_hot", "liveness", "strict"}


def pytest_addoption(parser: pytest.Parser) -> None:
    group = parser.getgroup("snappea", "checking P specs (snappea)")
    group.addoption("--snappea-strict", action="store_true", default=False,
                    help="Require relevant events for every selected P spec in each checked test.")
    group.addoption("--snappea-coverage", metavar="PATH", default=None,
                    help="Merge suite monitor coverage as JSON into PATH (or set SNAPPEA_COVERAGE). Without it the "
                         "summary is still printed; 'off' turns it off.")
    group.addoption(
        "--snappea",
        dest="snappea_mode",
        choices=_config.MODES,
        default=None,
        help="which tests check the P specs: auto (all), marked, off. Overrides SNAPPEA_MODE and [tool.snappea] mode.",
    )


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers",
        "snappea(skip=False, check_hot=False, strict=False): P spec checking for this test. skip=True turns it "
        "off, skip=[names] leaves out some specs, check_hot=True also fails on a spec left in a hot state, "
        "strict=True fails when a selected spec saw no relevant event.",
    )
    config.stash[_COVERAGE] = []
    config.stash[_ERROR] = None
    path = _coverage_path(config)
    config.stash[_COVERAGE_DEFAULT] = path is None and not _coverage_off(config)
    config.stash[_COVERAGE_PATH] = path
    try:
        cfg = _config.load(str(config.rootpath))
    except SetupError as e:
        config.stash[_CONFIG] = None
        config.stash[_ERROR] = str(e)
        return
    explicit = config.getoption("snappea_mode") or os.environ.get(_config.MODE_ENV)
    if not cfg.spec_files and cfg.path is None and not explicit:
        cfg.mode = "off"  # no [tool.snappea]: installed but not set up here
    else:
        cfg.mode = config.getoption("snappea_mode") or cfg.mode or "auto"
    config.stash[_CONFIG] = cfg


def _coverage_off(config: pytest.Config) -> bool:
    """`--snappea-coverage=off`, else `SNAPPEA_COVERAGE=off`."""
    option = config.getoption("snappea_coverage")
    return (option or os.environ.get(_config.COVERAGE_ENV)) == "off"


def _coverage_path(config: pytest.Config) -> Optional[Path]:
    """`--snappea-coverage` (relative to the rootdir), else `SNAPPEA_COVERAGE` (relative to the
    working directory); None without one, or when it is `off`."""
    if _coverage_off(config):
        return None
    option = config.getoption("snappea_coverage")
    if option:
        path = Path(option)
        return path if path.is_absolute() else config.rootpath / path
    env = os.environ.get(_config.COVERAGE_ENV)
    return Path(env).absolute() if env else None


def pytest_report_header(config: pytest.Config) -> Optional[str]:
    cfg = config.stash.get(_CONFIG, None)
    err = config.stash.get(_ERROR, None)
    if err:
        return f"snappea: configuration error: {err}"
    if cfg is None or cfg.mode == "off":
        return None
    return f"snappea: checking P specs from {', '.join(cfg.spec_files)} ({cfg.mode})"


def _output_root(config: pytest.Config) -> Optional[str]:
    """pytest's temporary folder for this run (kept for the last few runs), so snappea's
    output for a failed test is easy to find; None falls back to the system's."""
    factory = getattr(config, "_tmp_path_factory", None)
    if factory is None:
        return None
    try:
        d = factory.getbasetemp() / "snappea"
        d.mkdir(exist_ok=True)
        return str(d)
    except OSError:
        return None


def _settings(item: pytest.Item) -> Optional[dict]:
    """How to check `item`, or None to not check it."""
    cfg = item.config.stash.get(_CONFIG, None)
    if cfg is None or cfg.mode == "off":
        return None
    marks = list(item.iter_markers("snappea"))
    if cfg.mode == "marked" and not marks:
        return None
    skip: List[str] = []
    check_hot: Optional[bool] = None
    strict: Optional[bool] = None
    for m in reversed(marks):  # closest mark wins
        unknown = set(m.kwargs) - MARK_KEYS
        if unknown or m.args:
            raise pytest.UsageError(f"@pytest.mark.snappea takes skip=, check_hot= and strict=; got {m.args or ''} {m.kwargs}")
        s = m.kwargs.get("skip", False)
        if s is True:
            return None
        if s:
            skip.extend([s] if isinstance(s, str) else s)
        if "liveness" in m.kwargs:
            item.warn(pytest.PytestDeprecationWarning("@pytest.mark.snappea(liveness=...) is deprecated; use check_hot="))
            check_hot = bool(m.kwargs["liveness"])
        if "check_hot" in m.kwargs:
            check_hot = bool(m.kwargs["check_hot"])
        if "strict" in m.kwargs:
            strict = bool(m.kwargs["strict"])
    specs = [s for s in cfg.specs if s not in skip]
    if cfg.specs and not specs:
        return None  # every configured spec skipped
    if strict is None and cfg.strict is None:
        # Not strict by default: unlike an explicit check, the plugin checks every test,
        # and most tests emit no events for a given spec.
        strict = False
    return {
        "specs": specs,
        "exclude": [*cfg.exclude, *skip],
        "check_hot": check_hot,
        "strict": strict,
        "files": cfg.files(),
        "config": cfg,
    }


@pytest.hookimpl(tryfirst=True)
def pytest_runtest_setup(item: pytest.Item) -> None:
    """Starts recording before fixtures run, so events from setup count too."""
    err = item.config.stash.get(_ERROR, None)
    if err:
        raise pytest.UsageError(f"[tool.snappea]: {err}")
    item.stash[_RECORDER] = None
    settings = _settings(item)
    wants_fixture = "snappea_events" in getattr(item, "fixturenames", ())
    if settings is None and not wants_fixture:
        return
    rec = Recorder(
        settings["specs"] if settings else (),
        spec_files=settings["files"] if settings else [],
        exclude=settings["exclude"] if settings else (),
        check_hot=settings["check_hot"] if settings else None,
        strict=True if item.config.getoption("snappea_strict") else settings["strict"] if settings else None,
    )
    rec._cfg = settings["config"] if settings else None
    rec._default_artifacts = _output_root(item.config)
    rec._plugin_checks = settings is not None  # type: ignore[attr-defined]
    item.stash[_RECORDER] = rec.start()


@pytest.hookimpl(wrapper=True)
def pytest_runtest_call(item: pytest.Item) -> Generator[None, Any, Any]:
    """Checks the test's events when the test body is done, so a violation fails the
    test itself rather than its teardown."""
    try:
        result = yield
    except BaseException:
        rec = item.stash.get(_RECORDER, None)
        if rec is not None:
            rec.stop()
            rec._cleanup_owned()
        raise
    rec = item.stash.get(_RECORDER, None)
    if rec is not None:
        rec.stop()
        if getattr(rec, "_plugin_checks", False):
            __tracebackhide__ = True
            try:
                rec.check()
            finally:
                collect = item.config.stash[_COVERAGE_PATH] is not None or item.config.stash[_COVERAGE_DEFAULT]
                if rec.report is not None and "specs" in rec.report and collect:
                    item.config.stash[_COVERAGE].append(rec.report)
        else:
            rec._cleanup_owned()
    return result


def pytest_runtest_teardown(item: pytest.Item) -> None:
    rec = item.stash.get(_RECORDER, None)
    if rec is not None:  # setup failed before the test ran
        rec.stop()
        rec._cleanup_owned()


@pytest.fixture
def snappea_events(request: pytest.FixtureRequest) -> Recorder:
    """The recorder of this test: ``snappea_events.count("eEnter")``,
    ``snappea_events.events_named("eEnter")``, ``snappea_events.add_log(path, parse)``,
    ``snappea_events.env()`` for child processes."""
    rec = request.node.stash.get(_RECORDER, None)
    if rec is None:
        raise SetupError("snappea_events: the snappea plugin did not start a recorder for this test")
    return rec


@contextlib.contextmanager
def _locked(path: Path) -> Iterator[None]:
    """An exclusive lock on `path`.lock, held across processes (pytest-xdist workers,
    several pytest runs sharing SNAPPEA_COVERAGE)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(f"{path}.lock", "a+b") as f:
        if os.name == "nt":
            import msvcrt

            while True:
                try:
                    f.seek(0)
                    msvcrt.locking(f.fileno(), msvcrt.LK_LOCK, 1)  # retries for 10 s
                    break
                except OSError:
                    continue
            try:
                yield
            finally:
                f.seek(0)
                msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(f.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(f.fileno(), fcntl.LOCK_UN)


@pytest.hookimpl(tryfirst=True)
def pytest_sessionfinish(session: pytest.Session, exitstatus: int) -> None:
    """Merge this run's coverage into the file in core, under a lock; report errors
    before pytest chooses its process exit code."""
    config = session.config
    path = config.stash.get(_COVERAGE_PATH, None)
    if path is None:
        if config.stash.get(_COVERAGE_DEFAULT, False):
            _default_report(config)
        return
    try:
        with _locked(path):
            reports = list(config.stash[_COVERAGE])
            if path.is_file() and path.stat().st_size:
                try:
                    reports.insert(0, json.loads(path.read_text(encoding="utf-8")))
                except ValueError as e:
                    raise SetupError(f"{path} is not a coverage JSON file ({e}); delete it to start over") from None
            result = subprocess.run(
                [find_snappea(), "coverage", "merge-observe", "--stdin", "--format", "text", "--json-report", str(path)],
                input=json.dumps(reports), text=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60,
            )
        if result.returncode:
            raise SetupError(f"coverage merge exited {result.returncode}: {result.stdout}")
        config.stash[_COVERAGE_SUMMARY] = (True, result.stdout, path)
    except (SetupError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        config.stash[_COVERAGE_SUMMARY] = (False, str(error), path)
        session.exitstatus = pytest.ExitCode.TESTS_FAILED


def _default_report(config: pytest.Config) -> None:
    """Without a path: the merged summary of this run's checks, written nowhere. An xdist
    worker hands its reports to the controller, which prints the whole run's."""
    reports = config.stash[_COVERAGE]
    workeroutput = getattr(config, "workeroutput", None)
    if workeroutput is not None:
        workeroutput[_WORKER_KEY] = json.dumps(reports)
        return
    if not reports:
        return
    try:
        result = subprocess.run(
            [find_snappea(), "coverage", "merge-observe", "--stdin", "--format", "text"],
            input=json.dumps(reports), text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60,
        )
        if result.returncode:
            raise SetupError(f"coverage merge exited {result.returncode}: {result.stdout}")
        config.stash[_COVERAGE_SUMMARY] = (True, result.stdout, None)
    except (SetupError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        # reporting by default must not fail a run whose checks passed
        config.stash[_COVERAGE_SUMMARY] = (False, str(error), None)


@pytest.hookimpl(optionalhook=True)
def pytest_testnodedown(node: Any, error: Any) -> None:
    """pytest-xdist: collect a finished worker's coverage reports."""
    reports = getattr(node, "workeroutput", {}).get(_WORKER_KEY)
    if reports:
        node.config.stash[_COVERAGE].extend(json.loads(reports))


def pytest_terminal_summary(terminalreporter: Any, exitstatus: int, config: pytest.Config) -> None:
    summary = config.stash.get(_COVERAGE_SUMMARY, None)
    if summary is None:
        return
    success, output, path = summary
    terminalreporter.section("Spec coverage (all completed checks)")
    for line in output.splitlines():
        terminalreporter.write_line(line)
    if path is None:
        if success:
            terminalreporter.write_line("--snappea-coverage=PATH keeps it as JSON; =off turns this report off.")
        else:
            terminalreporter.write_line("Spec coverage could not be reported (the checks are unaffected).", yellow=True)
    elif success:
        terminalreporter.write_line(f"JSON: {path}")
    else:
        terminalreporter.write_line("Spec coverage reporting failed", red=True)
