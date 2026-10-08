# Real agent capture framework

Record an installed, already authenticated agent through an isolated **debug**
`tuic-remote --instance`. Never launch the desktop application. The driver uses
only Python's standard library and the existing HTTP API. It never performs login,
reads a credential store, installs an agent, or fabricates terminal output.

The agent uses the normal authenticated HOME by default, as authorized for these
captures. CLI-owned authentication/session state remains in its normal location;
scenario cwd and tool file writes stay in the throwaway output directory under
`~/Gits`. Use `--agent-home` for an already authenticated alternate HOME under
`~/Gits` if desired. The driver never reads/copies token stores or performs login.
The daemon always has a separate HOME, config, debug file vault and temporary
files inside the output directory. Release daemons are excluded because their
startup reads the native vault; supply the debug artifact from its build directory.

```sh
python3 -B scripts/agent_capture/run.py inventory
python3 -B scripts/agent_capture/run.py record \
  --daemon /absolute/path/to/debug/tuic-remote \
  --scenario scripts/agent_capture/codex-short.json \
  --output "$HOME/Gits/.tmp/agent-capture-1343/codex-001"
```

The output directory must not already exist. Each scenario selects an agent and
explicit CLI arguments (`args`, without shell expansion). Adapt these arguments
to the installed CLI version. Steps are:

- `prompt`, `question`, `approval`: `text` submitted through `/sessions/{id}/submit`,
  preserving backend agent-specific Enter handling. Question/approval are responses
  to actual CLI dialogs; they do not inject synthetic question or approval events.
- `question`/`approval` may instead carry `key`: `enter`, `escape` or an arrow
  (`up`, `down`, `left`, `right`). A short settling gap allows initial Ink dialog
  input handlers to attach after their first paint; no command text is sent.
- `wait`: `expect` contains `agent` and/or boolean `awaiting`; `timeout_secs` is
  bounded to 600 seconds. Agent states are `idle`, `working`, `awaiting_input`,
  `completed`. Place waits before responses to observe the real dialog. For a prose
  question, wait for idle as well as visible text before answering: a question
  can stream before the composer becomes available.
  A wait with `text` instead reads real terminal grid rows (startup prompts may
  render before state detection recognizes them). `replay_expect` can explicitly
  select a nonempty subset of `expect` for the byte-only oracle. Live idle timers
  cannot be inferred from output-only replay; capture submission/awaiting states
  and record the omitted timer dependency explicitly in the scenario.
- `interrupt`: send Ctrl+C through the PTY write endpoint.

Create separate scenarios for CLI-specific dialogs. Included smoke scenarios
cover Claude (Haiku), Codex (default), pi (Gemini Flash, tools disabled) and goose
(default local model, no extension profile). Claude first denies the observed
external-import prompt; it never accepts credentials or a login challenge.
Ensure expected response text is absent from the submitted prompt, so its echo
cannot count as success.
A missed state, detected login failure, rejected submission or process exit fails the
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

The resulting `<agent>-<name>.tcap`, provenance `.md`, and `.scenario.json` enter the existing 1342
filesystem corpus automatically. Scenario expected states must appear **in order**
in the production chunk replay, independently of the recorded golden. They are
never regenerated from parser output. Scenario captures also replay recorded
input through the production input FSM, preserving the real submit transition;
legacy output-only goldens remain unchanged. Live hook/timer-derived states may not be
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
| Claude | `~/.local/bin/claude` | Unverified: real CLI reports login expired; never logged in automatically |
| Codex | `/opt/homebrew/bin/codex` | Recorded and replayed real READY turn; fixture `codex-headless-short-20261008` |
| pi | `/opt/homebrew/bin/pi` | Recorded and replayed READY turn and real question/answer (Gemini 2.5 Flash); fixtures `pi-headless-short-20261008`, `pi-headless-question-20261008` |
| goose | `~/.local/bin/goose` | Unverified: the real banner prints the capture cwd, so a recording under `~/Gits` embeds the home path; re-record with a username-free cwd |
| Amp | Not found on PATH | Unverified |
| Cursor | `cursor-agent` not found on PATH | Unverified |
| Droid | Not found on PATH | Unverified |

The inventory command is a PATH check, not proof of authentication or CLI
behavior. The Codex fixture and the two pi fixtures passed the focused replay gate on 2026-10-08.
The real Claude capture embedded the home path through its CLAUDE.md import list,
so it is not committed; `reject_login` is tested with the observed wording instead.
Committed fixtures must not contain `/Users/` or `/home/`; promotion stores
`<output>/...` placeholders instead of the recording paths.

## Focused driver tests

```sh
scripts/with-test-tmp.sh python3 -B -m unittest discover -s scripts/agent_capture -p 'test_*.py'
```

These protect malformed scenario rejection, honest inventory, login-failure
rejection, the absence of home paths from committed capture fixtures, and lossless/non-overwriting promotion using
an existing real Codex capture. Live capture evidence is separately recorded in
the promoted fixture provenance and scenario metadata.

The `goose-question.json` probe remains unverified: goose rendered a real question
but TUIC kept `working` / `awaiting: false`, then rejected the answer submission
with HTTP 409. No successful question fixture is claimed for that probe.
