"""Validate and execute developer-authored scenarios against the real HTTP API."""
import math
import time

AGENTS = {
    "gemini": "gemini", "aider": "aider", "pi": "pi", "goose": "goose",
    "amp": "amp", "cursor": "cursor-agent", "droid": "droid",
}


def validate(scenario):
    if not isinstance(scenario, dict) or scenario.get("agent") not in AGENTS:
        raise ValueError("scenario.agent must name a supported capture agent")
    if not isinstance(scenario.get("args"), list) or not all(
        isinstance(arg, str) for arg in scenario["args"]
    ):
        raise ValueError("scenario.args must contain explicit CLI arguments")
    steps = scenario.get("steps")
    if not isinstance(steps, list) or not steps:
        raise ValueError("scenario.steps must be nonempty")
    expected = []
    for step in steps:
        if not isinstance(step, dict):
            raise ValueError("each scenario step must be an object")
        action = step.get("action")
        if action in ("prompt", "question", "approval"):
            if not isinstance(step.get("text"), str) or not step["text"].strip():
                raise ValueError(f"{action} requires text")
        elif action == "wait":
            seconds = step.get("timeout_secs", 60)
            if (isinstance(seconds, bool) or not isinstance(seconds, (int, float))
                    or not math.isfinite(seconds) or not 0 < seconds <= 600):
                raise ValueError("wait timeout_secs must be in (0, 600]")
            state = step.get("expect")
            if not isinstance(state, dict) or not state or set(state) - {"agent", "awaiting"}:
                raise ValueError("wait.expect requires agent and/or awaiting")
            if "agent" in state and state["agent"] not in ("idle", "working", "awaiting_input", "completed"):
                raise ValueError("expected agent must be idle, working, awaiting_input or completed")
            if "awaiting" in state and not isinstance(state["awaiting"], bool):
                raise ValueError("expected awaiting must be boolean")
            expected.append(state)
        elif action != "interrupt":
            raise ValueError(f"unknown scenario action: {action}")
    if not expected:
        raise ValueError("scenario requires at least one expected state")
    return expected


def run_steps(api, session_id, steps):
    observed = []
    for step in steps:
        action = step["action"]
        if action == "interrupt":
            api("POST", f"/sessions/{session_id}/write", {"data": "\x03"})
        elif action == "wait":
            deadline = time.monotonic() + step.get("timeout_secs", 60)
            while True:
                rows = api("GET", "/sessions")
                row = next((row for row in rows if row["session_id"] == session_id), None)
                if row is None:
                    raise RuntimeError("agent exited before the expected state")
                state = row.get("state") or {}
                snapshot = {"agent": state.get("agent_state"),
                            "awaiting": state.get("awaiting_input", False)}
                if all(snapshot.get(key) == value for key, value in step["expect"].items()):
                    observed.append(snapshot)
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"expected state was not reached: {step['expect']}")
                time.sleep(0.1)
        else:
            # Managed submit owns agent-specific split Enter writes; never append CR here.
            result = api("POST", f"/sessions/{session_id}/submit", {"input": step["text"]})
            if not result.get("submitted"):
                raise RuntimeError("agent rejected scenario submission")
    return observed
