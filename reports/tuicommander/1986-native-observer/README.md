# Native parked-shell observer — 1986-1236

These are scoped native evidence receipts, not a passing desktop latency result.
The original full evidence remains under `~/Gits/.tmp/results/desk-1986-visible-r4/`
and `desk-1986-visible-r5/`. The committed JSON preserves the repeated-snapshot
check, real integration-derived idle state, final stopped-process observations,
window visibility and cleanup. No production terminal text or desktop pixels are
included.

## Observer fixes

- `scripts/agent-test-visible.py` correlates evaluation results with a unique
  nonce. Identical consecutive snapshots no longer look like missing completion
  when the backend coalesces logs. A matching error is reported directly.
- `observer.py` sources the owned bash's real `$TUIC_SHELL_INTEGRATION` before the
  foreground child exits. Both the backend and frontend must report idle before
  the observer starts the standby check. It never sets idle or sends SIGSTOP.
- The wait for actual standby uses OS state only. It does not read session output,
  add a remote watcher or renew visibility. One synthetic terminal keydown is sent
  only after the stopped-state sample and visible-window precondition.

`observer.py` preserves the exercised r5 operations with a repository-relative
root and an explicit new evidence directory. It is an authorized macOS native
observer, not an unattended unit test. It builds and starts only `at-1986` on port
19877, uses that instance's Unix socket, deletes its owned session, and stops the
instance in `finally`. It does not take focus or use a denied show API.

```sh
python3 -B reports/tuicommander/1986-native-observer/observer.py \
  --evidence-dir "$HOME/Gits/.tmp/results/desk-1986-next"
```

Do not run it until the coordinator authorizes the next desktop check. At the
current handoff, the coordinator has identified the disappearing test window as
follow-up story **2095-0106** and has asked this peer to wait for that fix.

## Disposition

The nonce regression, native error propagation, integration-derived idle and real
standby were observed in both r4 and r5. In r4 a raw `Ω` resumed the exact stopped
shell group, but the document was hidden and no echo paint was observed. In r5 the
unlocked Mac had Slack frontmost, but the native test window reported invisible
before input, so the observer aborted without typing.

Neither run supplies a valid keyboard-to-painted-echo number. Neither is evidence
of an input-wake product defect. Physical keyboard/display latency, an authenticated
AI agent exit and Boss's phone are not proved. AC4 stays unchecked. Each cleanup
receipt records an empty session list, successful scoped stop and no remaining
worktree test executable.
