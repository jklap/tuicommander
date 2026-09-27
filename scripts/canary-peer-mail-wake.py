#!/usr/bin/env python3
"""Check that a real idle managed agent receives peer mail in its PTY."""

import json
import os
import sys
import time
import urllib.error
import urllib.request


BASE = os.environ.get("TUIC_CANARY_URL", "http://127.0.0.1:9877").rstrip("/")
AGENT = sys.argv[1] if len(sys.argv) > 1 else "claude"
CAPACITY = sys.argv[2:] == ["--capacity"]
if AGENT not in {"claude", "codex"} or (len(sys.argv) > 2 and not CAPACITY):
    sys.exit("usage: canary-peer-mail-wake.py [claude|codex] [--capacity]")


def request(path, body=None, headers=None, method=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        BASE + path,
        data=data,
        headers={"Content-Type": "application/json", **(headers or {})},
        method=method,
    )
    with urllib.request.urlopen(req, timeout=10) as response:
        raw = response.read()
        return json.loads(raw) if raw else None, response.headers


def mcp_call(sid, action, **args):
    result, _ = request(
        "/mcp",
        {"jsonrpc": "2.0", "id": action, "method": "tools/call",
         "params": {"name": "agent", "arguments": {"action": action, **args}}},
        {"Accept": "application/json, text/event-stream", "Mcp-Session-Id": sid},
    )
    if "error" in result:
        raise RuntimeError(result["error"])
    value = json.loads(result["result"]["content"][0]["text"])
    if "error" in value:
        raise RuntimeError(value["error"])
    return value


def initialize(extra_headers=None):
    initialized, headers = request(
        "/mcp",
        {"jsonrpc": "2.0", "id": 1, "method": "initialize",
         "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                    "clientInfo": {"name": "peer-mail-canary", "version": "1"}}},
        {"Accept": "application/json, text/event-stream", **(extra_headers or {})},
    )
    if "error" in initialized:
        raise RuntimeError(initialized["error"])
    return headers["Mcp-Session-Id"]


session_id = None
try:
    mcp_sid = initialize()
    mcp_call(mcp_sid, "register", name="peer-mail-canary")
    spawned, _ = request(
        "/sessions/agent",
        {"agent_type": AGENT, "cwd": os.getcwd(),
         "prompt": "Reply CANARY_READY once, then wait for a new message."},
    )
    session_id = spawned["session_id"]

    ready_by = time.monotonic() + 120
    while time.monotonic() < ready_by:
        sessions, _ = request("/sessions")
        recipient = next((item for item in sessions if item["session_id"] == session_id), None)
        state = recipient.get("state", {}) if recipient else {}
        if (recipient and recipient.get("tuic_session")
                and state.get("shell_state") == "idle"
                and state.get("agent_state") in {"idle", "completed"}):
            break
        time.sleep(1)
    else:
        raise RuntimeError(f"{AGENT} did not reach a bound idle composer in 120 s")

    sent = mcp_call(mcp_sid, "send", to=session_id, message="canary mail")
    if sent.get("delivery_path") != "wake_notification_and_inbox":
        raise RuntimeError(f"mail did not choose the PTY wake: {sent}")

    wake_by = time.monotonic() + 20
    while time.monotonic() < wake_by:
        output, _ = request(f"/sessions/{session_id}/output?format=log")
        observed = "\n".join(output.get("lines", []) + output.get("screen", []))
        if "[TUIC] message available" in observed and "agent action=inbox" in observed:
            print(f"PASS {AGENT}: peer mail wake appeared in PTY within 20 s")
            break
        time.sleep(1)
    else:
        raise RuntimeError(f"{AGENT} PTY did not show PEER_MAIL_WAKE within 20 s")

    if CAPACITY:
        # The reader is a second MCP connection bound to this disposable PTY.
        # It observes the same inbox without relying on the agent's reply time.
        reader_sid = initialize({"X-Tuic-Session": recipient["tuic_session"]})
        baseline = mcp_call(reader_sid, "inbox", since=0, limit=100)
        deadline = time.monotonic() + 90
        for index in range(100):
            if time.monotonic() >= deadline:
                raise RuntimeError("capacity canary timed out before 100 sends")
            mcp_call(mcp_sid, "send", to=session_id, message=f"capacity-{index}")
        read = mcp_call(reader_sid, "inbox", since=baseline["next_since"], limit=100)
        if read["count"] != 100:
            raise RuntimeError(f"capacity canary read {read['count']} of 100 messages")
        mcp_call(mcp_sid, "send", to=session_id, message="capacity-101")
        unread = mcp_call(reader_sid, "inbox", since=read["next_since"])
        if unread["count"] != 1 or unread["messages"][0]["content"] != "capacity-101":
            raise RuntimeError(f"101st message is not readable: {unread}")
        print("PASS: inbox accepted and returned mail after 100 read messages")
finally:
    if session_id:
        try:
            request(f"/sessions/{session_id}", method="DELETE")
        except (OSError, urllib.error.HTTPError):
            print(f"cleanup needed: session {session_id}", file=sys.stderr)
