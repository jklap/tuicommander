# Real agent capture framework

Record an installed, already authenticated agent through an isolated **debug**
`tuic-remote --instance`. Never launch the desktop application. The driver uses
only Python's standard library and the existing HTTP API. It never performs login,
reads a credential store, installs an agent, or fabricates terminal output.

Prepare an authenticated agent HOME under `~/Gits` through your normal manual
setup. The driver does not copy credentials from your normal HOME. Its daemon
HOME, config, debug file vault, temporary files and captures stay inside the new
output directory. Agent HOME is separate; the driver preserves it after the run.
No caller credential/config-directory environment overrides are inherited.
Choose a CLI configuration that uses this HOME and does not use native keychain
integration. Release daemons are excluded because their startup reads the native
vault. A debug artifact must be supplied from its build's `debug/` directory.

```sh
python3 -B scripts/agent_capture/run.py inventory
python3 -B scripts/agent_capture/run.py record \
  --daemon /absolute/path/to/debug/tuic-remote \
  --scenario scripts/agent_capture/scenario.example.json \
  --agent-home "$HOME/Gits/.tmp/capture-auth/pi" \
  --output "$HOME/Gits/.tmp/pi-capture-001"
```

The output directory must not already exist. Each scenario selects an agent and
explicit CLI arguments (`args`, without shell expansion). Adapt these arguments
to the installed CLI version. Steps are:

- `prompt`, `question`, `approval`: `text` submitted through `/sessions/{id}/submit`,
  preserving backend agent-specific Enter handling. Question/approval are responses
  to actual CLI dialogs; they do not inject synthetic question or approval events.
- `wait`: `expect` contains `agent` and/or boolean `awaiting`; `timeout_secs` is
  bounded to 600 seconds. Agent states are `idle`, `working`, `awaiting_input`,
  `completed`. Place waits before responses to observe the real dialog.
- `interrupt`: send Ctrl+C through the PTY write endpoint.

Create separate scenarios for CLI-specific dialogs. The example is a minimal
prompt/interrupt scenario, not evidence that pi supports a particular dialog.
A missed state, login requirement, rejected submission or process exit fails the
run and leaves `capture.json` marked **unverified**. The isolated daemon log and
partial raw recording remain available for diagnosis. Success records source
binary, original scenario, expected states, observed states and SHA-256.
Only the session and daemon created by the driver are stopped.

## Promotion and replay

Review the real recording for secrets and unrelated content before promotion.
Captures contain raw prompts/output; there is no automatic redaction of `.tcap`.
Promotion preserves the bytes and refuses truncated, changed, failed or duplicate
recordings:

```sh
python3 -B scripts/agent_capture/run.py promote \
  "$HOME/Gits/.tmp/pi-capture-001/capture.json" --name simple-turn-20261008
```

The resulting `<agent>-<name>.tcap` , provenance `.md`, and `.scenario.json` enter the existing 1342
filesystem corpus automatically. Scenario expected states must appear **in order**
in the production chunk replay, independently of the recorded golden. They are
never regenerated from parser output. Live hook/timer-derived states may not be
reproducible from bytes: promotion/replay must expose that gap, not silently
change the expectation. Run the coordinator-approved, targeted oracle command
through the required background/build-slot wrappers:

```sh
scripts/replay-oracle.sh regenerate
# Review new golden events and corpus manifest; subsequent verification:
scripts/replay-oracle.sh verify
```

Regeneration records the production trace and still enforces scenario expectations.
Do not claim a capture verified until replay passes. Commit capture, provenance,
scenario metadata and the generated oracle files together.

## Mac inventory (2026-10-08)

| Agent | Installation | Capture verification |
| --- | --- | --- |
| Gemini | Not found on PATH | Unverified |
| aider | Not found on PATH | Unverified |
| pi | `/opt/homebrew/bin/pi` | Unverified; authenticated isolated HOME not supplied |
| goose | `~/.local/bin/goose` | Unverified; authenticated isolated HOME not supplied |
| Amp | Not found on PATH | Unverified |
| Cursor | `cursor-agent` not found on PATH | Unverified |
| Droid | Not found on PATH | Unverified |

Inventory is a PATH check, not proof of authentication or CLI behavior. No new
agent fixture is included until a real scenario succeeds and its replay passes.

## Focused driver tests

```sh
scripts/with-test-tmp.sh python3 -B -m unittest discover -s scripts/agent_capture -p 'test_*.py'
```

These protect malformed scenario rejection, honest inventory and lossless,
non-overwriting promotion using an existing real Codex capture. They do not claim
an installed agent was exercised.
