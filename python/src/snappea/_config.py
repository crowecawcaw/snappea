"""`[tool.snappea]` in pyproject.toml, and the environment variables that override it."""

from __future__ import annotations

import glob
import os
from dataclasses import dataclass, field, replace
from functools import lru_cache
from typing import List, Optional

try:
    import tomllib  # type: ignore[import-not-found]
except ImportError:  # Python < 3.11
    import tomli as tomllib  # type: ignore[no-redef]

from ._errors import SetupError

MODES = ("auto", "marked", "off")
UNKNOWN_EVENTS = ("ignore", "warn", "error")
SPEC_FILES_ENV = "SNAPPEA_SPEC_FILES"
ARTIFACTS_ENV = "SNAPPEA_ARTIFACTS"
MODE_ENV = "SNAPPEA_MODE"
COVERAGE_ENV = "SNAPPEA_COVERAGE"
#: Seconds a check may take.
DEFAULT_TIMEOUT = 30.0

KEYS = ("spec_files", "specs", "exclude", "strict", "check_hot", "unknown_events", "timeout", "artifacts", "coverage", "mode")


class ConfigError(SetupError):
    """Bad ``[tool.snappea]`` settings or environment: a :class:`SetupError` of kind ``config``."""

    def __init__(self, message: str):
        super().__init__(message, "config")


@dataclass
class Config:
    #: Glob patterns of the P files holding the specs, relative to `root`.
    spec_files: List[str] = field(default_factory=list)
    #: Specs to check; empty means every spec in the spec files.
    specs: List[str] = field(default_factory=list)
    #: Specs left out of the every-spec selection.
    exclude: List[str] = field(default_factory=list)
    #: "auto": every test; "marked": tests marked `@pytest.mark.snappea`; "off". None: not set.
    mode: Optional[str] = None
    #: Folder of the pyproject.toml the settings came from.
    root: str = "."
    #: The pyproject.toml, if one had a [tool.snappea] table.
    path: Optional[str] = None
    #: Require relevant events for every selected spec. None: the default.
    strict: Optional[bool] = None
    #: Also fail on a spec left in a hot state. None: the default (false).
    check_hot: Optional[bool] = None
    #: "ignore", "warn" or "error". None: the default ("error").
    unknown_events: Optional[str] = None
    #: Seconds a check may take. None: the default.
    timeout: Optional[float] = None
    #: Folder to keep failed checks' events and output in (absolute). None: a temp folder.
    artifacts: Optional[str] = None
    #: Suite coverage JSON path (absolute), or "off". None: print the summary only.
    coverage: Optional[str] = None

    def files(self) -> List[str]:
        """The P files, expanded and sorted."""
        out: List[str] = []
        for pat in self.spec_files:
            full = pat if os.path.isabs(pat) else os.path.join(self.root, pat)
            matches = sorted(glob.glob(full, recursive=True))
            if not matches:
                raise ConfigError(f"spec_files: {pat!r} matches no files (looked in {self.root})")
            for m in matches:
                if os.path.isdir(m):
                    matches_in = sorted(glob.glob(os.path.join(m, "**", "*.p"), recursive=True))
                    out.extend(x for x in matches_in if x not in out)
                elif m not in out:
                    out.append(m)
        return out


def check_unknown_events(value: object, where: str) -> str:
    if value not in UNKNOWN_EVENTS:
        raise ConfigError(f"{where} must be one of {', '.join(UNKNOWN_EVENTS)}, not {value!r}")
    return value  # type: ignore[return-value]


def check_mode(value: object, where: str) -> str:
    if value not in MODES:
        raise ConfigError(f"{where} must be one of {', '.join(MODES)}, not {value!r}")
    return value  # type: ignore[return-value]


def parse(table: dict, root: str, path: Optional[str]) -> Config:
    def str_list(name: str) -> List[str]:
        v = table.get(name, [])
        if isinstance(v, str):
            v = [v]
        if not isinstance(v, list) or not all(isinstance(x, str) for x in v):
            raise ConfigError(f"[tool.snappea] {name} must be a list of strings")
        return v

    def opt_bool(name: str) -> Optional[bool]:
        v = table.get(name)
        if v is not None and not isinstance(v, bool):
            raise ConfigError(f"[tool.snappea] {name} must be a boolean")
        return v

    unknown = set(table) - set(KEYS)
    if unknown:
        raise ConfigError(f"[tool.snappea]: unknown setting(s) {', '.join(sorted(unknown))}")
    mode = table.get("mode")
    if mode is not None:
        check_mode(mode, "[tool.snappea] mode")
    unknown_events = table.get("unknown_events")
    if unknown_events is not None:
        check_unknown_events(unknown_events, "[tool.snappea] unknown_events")
    timeout = table.get("timeout")
    if timeout is not None and (isinstance(timeout, bool) or not isinstance(timeout, (int, float)) or timeout <= 0):
        raise ConfigError("[tool.snappea] timeout must be a positive number of seconds")
    artifacts = table.get("artifacts")
    if artifacts is not None:
        if not isinstance(artifacts, str):
            raise ConfigError("[tool.snappea] artifacts must be a path")
        artifacts = os.path.join(root, artifacts)
    coverage = table.get("coverage")
    if coverage is not None:
        if not isinstance(coverage, str) or not coverage:
            raise ConfigError('[tool.snappea] coverage must be a path or "off"')
        if coverage != "off":
            coverage = os.path.join(root, coverage)
    return Config(
        spec_files=str_list("spec_files"),
        specs=str_list("specs"),
        exclude=str_list("exclude"),
        mode=mode,
        root=root,
        path=path,
        strict=opt_bool("strict"),
        check_hot=opt_bool("check_hot"),
        unknown_events=unknown_events,
        timeout=float(timeout) if timeout is not None else None,
        artifacts=artifacts,
        coverage=coverage,
    )


def load(start: str) -> Config:
    """The settings of the nearest pyproject.toml with a [tool.snappea] table, searching
    upwards from `start`, with environment variables applied: `SNAPPEA_SPEC_FILES` (paths
    separated by os.pathsep), `SNAPPEA_ARTIFACTS` and `SNAPPEA_MODE`. Relative paths in them
    resolve against the working directory."""
    cfg = replace(_load(os.path.abspath(start)))
    cwd = os.getcwd()
    env = os.environ.get(SPEC_FILES_ENV)
    if env:
        cfg.spec_files = [os.path.join(cwd, p) for p in env.split(os.pathsep) if p]
    env = os.environ.get(ARTIFACTS_ENV)
    if env:
        cfg.artifacts = os.path.join(cwd, env)
    env = os.environ.get(MODE_ENV)
    if env:
        cfg.mode = check_mode(env, MODE_ENV)
    return cfg


@lru_cache(maxsize=None)
def _load(start: str) -> Config:
    d = start
    while True:
        p = os.path.join(d, "pyproject.toml")
        if os.path.isfile(p):
            with open(p, "rb") as f:
                try:
                    data = tomllib.load(f)
                except tomllib.TOMLDecodeError as e:
                    raise ConfigError(f"{p}: {e}") from None
            table = data.get("tool", {}).get("snappea")
            if table is not None:
                return parse(table, d, p)
        parent = os.path.dirname(d)
        if parent == d:
            return Config(root=start)
        d = parent
