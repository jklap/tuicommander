"""Locations shared by the maintainer-only native verification scripts.

Scratch and runtime files come from `host_temp_dir()`: never `$HOME`. The
other helpers only LOCATE tools and a coordination file that live outside this
repository in the maintainer's own layout. These scripts read or execute them
and never write them, and each location has an environment override.
"""
import os
from pathlib import Path
import tempfile


def host_temp_dir() -> Path:
    """The caller's temp dir, also inside scripts/with-test-tmp.sh (its per-run
    TMPDIR is too long for a Unix socket; the wrapper exports the original as
    TUIC_TEST_HOST_TMPDIR)."""
    return Path(os.environ.get("TUIC_TEST_HOST_TMPDIR") or tempfile.gettempdir())


def _external(variable: str, default: str) -> Path:
    value = os.environ.get(variable)
    return Path(value).expanduser() if value else Path.home() / "Gits" / default


def build_slot() -> Path:
    """The orchestrator's build-slot runner; callers run it only when present."""
    return _external("TUIC_BUILD_SLOT", "personal/orchestrator/tools/build/build-slot.sh")


def build_freeze() -> Path:
    """The BUILD_FREEZE marker the maintainer's orchestrator creates to pause
    native builds. Read only, never created or removed here."""
    return _external("TUIC_BUILD_FREEZE_FILE", ".tmp/BUILD_FREEZE")


def browser_stealth_wrapper() -> Path:
    """The maintainer's agent-browser stealth wrapper."""
    return _external("TUIC_AB_STEALTH", "personal/brainstorming/tools/browser-stealth/ab-stealth.sh")
