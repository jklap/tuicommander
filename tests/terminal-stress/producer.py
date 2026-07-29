#!/usr/bin/env python3
"""Emit deterministic terminal stress scenarios to stdout."""

from __future__ import annotations

import argparse
import os
import termios
import time
import tty

BSU = b"\x1b[?2026h"
ESU = b"\x1b[?2026l"
CHUNK_SCHEDULES = (
    (1, 2, 3, 5, 8, 13, 21),
    (127, 4, 31, 2, 255, 7),
    (9, 1, 1, 1, 64, 3, 17, 5),
)


def record(index: int) -> bytes:
    return f"REC-{index:04d}:abcdefghijklmnopqrstuvwxyz0123456789".encode()


def write_fragmented(payload: bytes, frame: int) -> None:
    schedule = CHUNK_SCHEDULES[frame % len(CHUNK_SCHEDULES)]
    offset = 0
    step = 0
    while offset < len(payload):
        end = min(offset + schedule[step % len(schedule)], len(payload))
        os.write(1, payload[offset:end])
        offset = end
        step += 1


def emit(scenario: str, count: int, marker: str | None = None) -> None:
    for index in range(count):
        complete = record(index)
        partial = complete[: 8 + index % 17]

        if scenario == "progressive":
            # Model token streaming across separate synchronized frames: the
            # partial row becomes visible before a later frame replaces it.
            write_fragmented(BSU + partial + ESU, index)
            time.sleep(0.001)
            tail = BSU + b"\r\x1b[2K" + complete + b"\r\n" + ESU
            write_fragmented(tail, index + 1)
        elif scenario == "timeout" and index % 97 == 0:
            write_fragmented(BSU + partial, index)
            # Intentionally exceed the current 150 ms synchronized-update
            # timeout. The remainder must replace the partial row, not preserve
            # it as an extra logical record.
            time.sleep(0.18)
            tail = b"\r\x1b[2K" + complete + b"\r\n" + ESU
            write_fragmented(tail, index + 1)
        else:
            frame = BSU + partial + b"\r\x1b[2K" + complete + b"\r\n" + ESU
            write_fragmented(frame, index)

        if index % 11 == 0:
            time.sleep(0.001)

    marker = marker or scenario
    os.write(1, f"\r\nTUIC_STRESS_DONE:{marker}:{count}\r\n".encode())


def emit_slash_pressure(count: int) -> None:
    original = termios.tcgetattr(0)
    try:
        # Consume one slash without echoing it into the terminal grid. TUIC's
        # input FSM still enters slash mode, so every subsequent PTY chunk
        # exercises slash-menu parsing exactly like the captured log storm.
        tty.setraw(0)
        os.write(1, b"TUIC_STRESS_READY:slash-pressure\r\n")
        if os.read(0, 1) != b"/":
            raise RuntimeError("slash-pressure handshake did not receive slash")
        emit("atomic", count, marker="slash-pressure")
    finally:
        termios.tcsetattr(0, termios.TCSADRAIN, original)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scenario",
        choices=("atomic", "progressive", "timeout", "slash-pressure"),
        required=True,
    )
    parser.add_argument("--count", type=int, default=2000)
    args = parser.parse_args()
    if args.count < 1 or args.count > 9999:
        parser.error("--count must be between 1 and 9999")
    if args.scenario == "slash-pressure":
        emit_slash_pressure(args.count)
    else:
        emit(args.scenario, args.count)


if __name__ == "__main__":
    main()
