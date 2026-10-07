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
    # Wording observed from the real Claude CLI; the recording itself embedded the
    # home path (CLAUDE.md import list), so it cannot be committed.
    def test_real_login_failure_cannot_be_promoted_as_success(self):
        data = (b"\x1b[1m\xe2\x8f\xb5\xe2\x8f\xb5 auto mode on (shift+tab to cycle)"
                b" Not logged in \xc2\xb7 Run /login\r\n")
        with self.assertRaisesRegex(RuntimeError, "requires login"):
            run.reject_login(data.decode())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src-tauri/src/fixtures/agent_prompts").mkdir(parents=True)
            capture = root / "login.tcap"
            capture.write_bytes(b"TUICCAP2\n" + b"\0" * 4 + bytes([0]) + b"\0" * 8
                                + len(data).to_bytes(4, "little") + data)
            metadata = {"agent": "claude", "status": "captured",
                        "expected_states": [{"awaiting": False}],
                        "sha256": hashlib.sha256(capture.read_bytes()).hexdigest()}
            with patch("run.ROOT", root), self.assertRaisesRegex(RuntimeError, "requires login"):
                run.promote(capture, metadata, "must-remain-unverified")
            self.assertEqual(list((root / "src-tauri/src/fixtures/agent_prompts").iterdir()), [])

    # Catches: provenance metadata leaks the recorder's home directory into a public repo.
    def test_promotion_replaces_recording_paths_with_placeholders(self):
        metadata = {"daemon_health": {"socket_path": "/Users/someone/Gits/.tmp/run/tuic.sock"},
                    "capture": "/Users/someone/Gits/.tmp/run/raw/id.tcap"}
        shown = run.portable(metadata)
        self.assertEqual(shown["daemon_health"]["socket_path"], "<output>/tuic.sock")
        self.assertEqual(shown["capture"], "<output>/raw/id.tcap")
        self.assertIn("/Users/", metadata["capture"])

    # Catches: a re-recorded fixture or scenario file commits the recorder's home path.
    def test_committed_capture_fixtures_hold_no_home_paths(self):
        prompts = run.ROOT / "src-tauri/src/fixtures/agent_prompts"
        scenarios = sorted(prompts.glob("*.scenario.json"))
        self.assertTrue(scenarios, "no promoted scenario fixtures found")
        files = sorted(Path(__file__).parent.glob("*.json"))
        files += sorted((Path(__file__).parent / "fixtures").glob("*"))
        for scenario in scenarios:
            stem = scenario.name[: -len(".scenario.json")]
            files += [scenario, *sorted(prompts.glob(stem + ".*"))]
        for path in files:
            data = path.read_bytes()
            for marker in (b"/Users/", b"/home/"):
                self.assertNotIn(marker, data, f"{path.name} embeds {marker.decode()}")

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
