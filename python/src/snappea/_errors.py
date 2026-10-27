"""Errors and warnings shared by the recorder, the binary lookup and the configuration."""

from __future__ import annotations

import os
import sys
import warnings
from typing import Any, Optional

#: Values of :attr:`SetupError.kind`.
KINDS = ("binary_missing", "config", "parser", "validation", "unknown_event", "out_of_order", "timeout")


class SetupError(Exception):
    """The check could not run, or its input was bad: anything but a broken spec.

    ``kind`` says why, for assertions without matching message text:

    * ``binary_missing``: no ``snappea`` binary
    * ``config``: bad options or ``[tool.snappea]``, missing spec files, P compile errors,
      an unknown spec name
    * ``parser``: an event line or payload snappea cannot read (including truncated lines)
    * ``validation``: ``strict`` found a selected spec with no relevant event
    * ``unknown_event``: an event name no P file declares, under ``unknown_events="error"``
    * ``out_of_order``: live input out of order
    * ``timeout``: the checker ran longer than ``timeout``
    """

    def __init__(self, message: str, kind: str = "config", *, output_dir: Optional[str] = None):
        super().__init__(message)
        #: One of :data:`KINDS` (or a newer kind the core reports).
        self.kind = kind
        #: The error text.
        self.message = message
        #: Folder with the events and snappea's output, if the check got that far.
        self.output_dir = output_dir


class StrictDefaultWarning(DeprecationWarning):
    """Deprecated and no longer emitted; kept so existing references keep working.

    It warned, for one release, that a check with named specs would fail under the new
    ``strict`` default. That default is now in force: such a check raises
    :class:`SetupError` with ``kind == "validation"``. Pass ``strict=False`` (or set
    ``[tool.snappea] strict = false``) when a spec may rightly see no events.
    """

    def __init__(self, error: SetupError):
        super().__init__(error.message)
        self.error = error


def warn(message: Any, category: Any = None) -> None:
    """`warnings.warn`, attributed to the first caller outside this package."""
    pkg = os.path.dirname(os.path.abspath(__file__))
    level, frame = 2, sys._getframe(1)
    while frame is not None and os.path.abspath(frame.f_code.co_filename).startswith(pkg + os.sep):
        frame, level = frame.f_back, level + 1
    warnings.warn(message, category, stacklevel=level)
