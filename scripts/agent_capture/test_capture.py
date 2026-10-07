"""Focused capture contract tests; no synthetic agent transcripts."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import run
from scenario import validate


class CaptureContractTests(unittest.TestCase):
    # Catches: malformed scripts launch a CLI before discovering unusable expectations.
    def test_invalid_scenario_is_rejected_before_launch(self):
        base = {"agent": "pi", "args": [], "steps": [
            {"action": "wait", "expect": {"agent": "working"}}]}
        self.assertEqual(validate(base), [{"agent": "working"}])
        dialog = {**base, "agent": "claude", "steps": [
            {"action": "wait", "text": "Allow external CLAUDE.md file imports?"},
            {"action": "approval", "key": "enter"}, *base["steps"]]}
        self.assertEqual(validate(dialog), [{"agent": "working"}])
        for steps in [[], [{"action": "prompt", "text": ""}],
                      [{"action": "wait", "expect": {}}],
                      [{"action": "wait", "expect": {"awaiting": "false"}}],
                      [{"action": "wait", "expect": {"agent": "working"}, "timeout_secs": float("nan")}],
                      [{"action": "shell", "text": "anything"}],
                      [{"action": "approval", "key": "unknown"}],
                      [{"action": "wait", "expect": {"agent": "working"}, "replay_expect": {"agent": "idle"}}]]:
            with self.subTest(steps=steps), self.assertRaises(ValueError):
                validate({**base, "steps": steps})

    # Catches: a missing binary is reported as verified or gets a fabricated capture.
    def test_missing_cli_remains_unverified(self):
        with patch("run.shutil.which", return_value=None):
            inventory = run.inventory()
        self.assertTrue(inventory)
        self.assertTrue(all(row == {"binary": None, "status": "unverified",
                                    "reason": "not installed"} for row in inventory.values()))

    # Catches: a login error looks like an idle successful turn and gets promoted.
    def test_real_login_failure_cannot_be_promoted_as_success(self):
        failure = Path(__file__).parent / "fixtures/claude-login-expired.tcap"
        data = failure.read_bytes()
        metadata = {"agent": "claude", "status": "captured",
                    "expected_states": [{"awaiting": False}],
                    "sha256": hashlib.sha256(data).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src-tauri/src/fixtures/agent_prompts").mkdir(parents=True)
            with patch("run.ROOT", root), self.assertRaisesRegex(RuntimeError, "requires login"):
                run.promote(failure, metadata, "must-remain-unverified")
            self.assertFalse(list(root.rglob("*.tcap")))

    # Catches: changed or failed recordings overwrite previously reviewed evidence.
    def test_promotion_preserves_real_bytes_and_refuses_overwrite_and_tampering(self):
        fixture = run.ROOT / "src-tauri/src/fixtures/agent_prompts/codex-narrow-ink-replay-20260925.tcap"
        real_bytes = fixture.read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src-tauri/src/fixtures/agent_prompts").mkdir(parents=True)
            capture = root / "real.tcap"
            capture.write_bytes(real_bytes)
            metadata = {"agent": "codex", "status": "captured",
                        "expected_states": [{"awaiting": True}],
                        "sha256": hashlib.sha256(real_bytes).hexdigest()}
            with patch("run.ROOT", root):
                dest = run.promote(capture, metadata, "recorded")
                self.assertEqual(dest.read_bytes(), real_bytes)
                self.assertIn(f"SHA-256: `{metadata['sha256']}`", dest.with_suffix(".md").read_text())
                self.assertEqual(json.loads(dest.with_suffix(".scenario.json").read_text()), metadata)
                with self.assertRaises(ValueError):
                    run.promote(capture, metadata, "recorded")
                with self.assertRaises(ValueError):
                    run.promote(capture, {**metadata, "status": "unverified"}, "failed")
                capture.write_bytes(real_bytes[:-1])
                truncated = {**metadata, "sha256": hashlib.sha256(capture.read_bytes()).hexdigest()}
                with self.assertRaises(ValueError):
                    run.promote(capture, truncated, "truncated")
                capture.write_bytes(real_bytes + b"changed")
                with self.assertRaises(ValueError):
                    run.promote(capture, metadata, "changed")
                self.assertFalse(dest.with_name("codex-changed.tcap").exists())


if __name__ == "__main__":
    unittest.main()
