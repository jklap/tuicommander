"""Focused launcher contracts. No test starts a desktop application."""
import json
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent


class AgentTestPlanTests(unittest.TestCase):
    def plan(self, *arguments, **environment):
        env = {**os.environ, **environment}
        env.pop("CARGO_TARGET_DIR", None)
        result = subprocess.run(
            ["make", "--no-print-directory", "agent-test", "AGENT_TEST_DRY_RUN=1", *arguments],
            cwd=ROOT, env=env, text=True, capture_output=True, check=True,
        )
        return json.loads(result.stdout)

    def test_dry_run_cannot_reuse_daily_config_binary_or_listener(self):
        plan = self.plan(TUIC_APP_INSTANCE="", TUIC_PORT="9876", TUIC_DEV_BIN_DIR="/unsafe")
        self.assertEqual(plan["environment"]["TUIC_APP_INSTANCE"], "agent-test")
        self.assertEqual(plan["environment"]["TUIC_PORT"], "19876")
        self.assertTrue(plan["config_dir"].endswith("/com.tuic.commander/instances/agent-test"))
        self.assertEqual(Path(plan["binary"]).name, "tuicommander-agent-test")
        self.assertTrue(Path(plan["binary"]).is_relative_to(ROOT / ".tmp"))
        self.assertEqual(plan["listener"], "127.0.0.1:19876 (no port fallback)")
        self.assertEqual(plan["disabled"], ["global shortcuts", "dictation/microphone", "Telegram", "updater", "global MCP install", "global CLI mutations", "global agent hooks", "deep links", "sleep inhibition"])
        self.assertIn("tauri/custom-protocol", plan["cargo"])
        self.assertNotIn("dictation", plan["cargo"])
        self.assertNotIn("CARGO_TARGET_DIR", plan["environment"])
        self.assertFalse(Path(plan["binary"]).exists(), "dry run must not copy/build the executable")

    def test_overrides_remain_named_and_use_a_separate_socket(self):
        default = self.plan()
        other = self.plan("AGENT_TEST_INSTANCE=scope-check", "AGENT_TEST_PORT=19877")
        self.assertEqual(other["environment"]["TUIC_APP_INSTANCE"], "scope-check")
        self.assertEqual(other["environment"]["TUIC_PORT"], "19877")
        self.assertNotEqual(default["socket"], other["socket"])
        self.assertLess(len(other["socket"].encode()), 104)

    def test_static_wiring_keeps_guards_before_machine_wide_effects(self):
        # Static audit requested by the story: dropping a runtime guard must fail
        # even though this peer cannot start the desktop app to observe OS effects.
        guarded = [
            ("global_hotkey.rs", "pub fn register(", "app.global_shortcut().register"),
            ("dictation/ownership.rs", "pub fn acquire(", "let open ="),
            ("telegram/native.rs", "pub(crate) fn start(", "super::settings::paths()"),
            ("telegram/settings.rs", "pub(crate) async fn change_owner_with_phase(", "ensure_directory(&paths)"),
            ("agent_mcp.rs", "fn launch_owns_agent_configs(", 'std::env::var("TUIC_MCP_CONFIG_OWNER")'),
            ("agent_mcp.rs", "fn install_spec(", "ensure_spec_entry("),
            ("updater.rs", "pub async fn check_update_channel(", "get_channel_config("),
        ]
        for file, signature, effect in guarded:
            with self.subTest(file=file, signature=signature):
                source = (ROOT / "src-tauri/src" / file).read_text().split(signature, 1)[1]
                self.assertLess(source.index("crate::agent_test::active()"), source.index(effect))
        source = (ROOT / "src-tauri/src/lib.rs").read_text()
        updater_setup = source.split('.setup(move |app| {', 1)[1]
        self.assertLess(updater_setup.index('if !agent_test::active()'), updater_setup.index('.plugin(tauri_plugin_updater'))
        listener = (ROOT / "src-tauri/src/mcp_http/mod.rs").read_text()
        self.assertRegex(listener, r'let host = if crate::agent_test::active\(\)\s*\{\s*"127.0.0.1"')
        self.assertRegex(listener, r'let attempts = if base_port == 0 \|\| crate::agent_test::active\(\)\s*\{\s*1')
        policy = (ROOT / "src-tauri/src/agent_test.rs").read_text()
        self.assertIn('cfg!(feature = "dictation")', policy)
        self.assertIn('cfg!(debug_assertions)', policy)

    def test_invalid_overrides_fail_closed_instead_of_using_production(self):
        for argument in ["AGENT_TEST_INSTANCE=", "AGENT_TEST_INSTANCE=default", "AGENT_TEST_INSTANCE=../escape", "AGENT_TEST_PORT=9876", "AGENT_TEST_PORT=0", "AGENT_TEST_PORT=65536"]:
            with self.subTest(argument=argument):
                result = subprocess.run(["make", "--no-print-directory", "agent-test", "AGENT_TEST_DRY_RUN=1", argument], cwd=ROOT, text=True, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("agent-test:", result.stderr)


if __name__ == "__main__":
    unittest.main()
