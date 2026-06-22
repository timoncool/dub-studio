"""Injectable progress reporting. The engine emits ProgressEvent(stage, pct, msg); apps pass any callback
(SSE push, log, no-op). default_logger reproduces the original [dub] stdout so nothing visibly changes."""
from typing import Callable, Optional, TypedDict


class ProgressEvent(TypedDict, total=False):
    stage: str
    pct: Optional[float]
    msg: str


Progress = Callable[[ProgressEvent], None]


def default_logger(ev: ProgressEvent) -> None:
    msg = ev.get("msg", "")
    stage = ev.get("stage") or ""
    print(f"[dub] {stage + ' ' if stage else ''}{msg}".rstrip())
