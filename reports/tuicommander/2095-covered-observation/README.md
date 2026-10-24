# Story 2095: covered agent-test regression

## Contract and scope

The dedicated agent-test document must stay visible and run animation frames
for five minutes under an opaque covering window. It must not become frontmost,
take keyboard focus, or move from its compact corner. The observer reads the
actual WebKit document, Tauri window and macOS window server through the test
instance's Unix socket and read-only OS queries. It never drives the daily app.

## Recovered RED

The previous implementer recorded the RED before the production fix at
`ec11bc320e`. The story worklog records this command and pre-fix revision:

```sh
python3 -B scripts/agent-test-visible.py --instance at-cc2095 --port 19963 \
  --duration 60 --cover \
  --evidence-dir "$HOME/Gits/.tmp/results/cc-2095-red2"
```

The observer `agent_test_window_renders_visible_without_focus` failed. Before
covering, the document was visible and the terminal painted real text. With the
other process's opaque layer-25 panel over the layer-5 test window, every
retained sample reported a hidden document and zero animation frames, through
60,034 ms. Native visibility remained true and applicationHidden remained false.
This isolates a covered WebKit page rather than an application hide or close.
The same run also showed the test application becoming frontmost without a key
window. See `red-observer.json` and `red-presentation.json`.

The saved first GREEN kept the document visible but had a 5,374 ms frame gap.
The next two attempts were interrupted and are not passing evidence. Their raw
evidence remains under `~/Gits/.tmp/results/cc-2095-green*`.

## Fix boundaries

Only the dedicated binary enables `agent_test::active()`. The launch policy
sets ActivationPolicy::Prohibited before Tauri run, not in the later setup hook.
Ready calls `present_without_focus` only when that policy is active; the daily
app still calls `ensure_window_visible`. The test WebView disables WebKit's
window-occlusion detection using a runtime-checked selector. A process-lifetime
activity prevents App Nap during checks while permitting idle system sleep.
The frame-gap observation does not itself prove that App Nap caused the gaps.
The ordinary application's activation, occlusion and sleep policy are unchanged.

Adjacent cases checked: uncovered native paint before the cover; covered native
visibility distinct from document visibility; application activation distinct
from native/document keyboard focus; daily-runtime exclusion by the active guard.
No extra compatibility path or shared application setting was added.

## Final native GREEN

On 2026-10-20 the recovery agent ran the final worktree implementation:

```sh
python3 -B scripts/agent-test-visible.py --instance at-cc2095 --port 19963 \
  --duration 300 --cover \
  --evidence-dir "$HOME/Gits/.tmp/results/fix-2095-recovered-green"
```

Result: **PASS**, exit 0. The launcher built the native macOS executable through
build-slot and mbx; no installed executable was replaced. The observation
retained 59 samples over 300,157 ms and 9,791 animation frames, with a maximum
frame gap of 319 ms. Every sample had a visible document and native window,
no document/native focus, unchanged geometry and activation policy 2
(Prohibited). The test PID was never frontmost. iTerm2 was frontmost before
launch and at readiness; retained samples also include loginwindow. This is not
proof of a continuously unlocked desktop or a fixed 60 fps cadence.

The cover was an opaque non-activating accessory panel from another process,
layer 25, enclosing the full test window. Its initial window-server geometry is
in `green-observer.json`. The observer terminates it after observation. The
640x420 logical test window stayed at the same corner throughout. Canvas-only
PNGs remain in the raw evidence directory; no desktop pixels were captured.

Cleanup deleted the throwaway session, successfully requested
`make agent-test-stop`, and found no remaining worktree executable. The
supervisor's exit 2 follows intentional child termination, not a cleanup failure.
The transient AppHandle-not-initialized setup response was retried before the
observation began. See `green-cleanup.json`.

## Evidence and remaining gates

The JSON files here retain native/document measurements and cleanup receipts.
Other applications' window rows are removed; foreground identities remain.
Original unfiltered files and build log remain in the raw evidence directories.

The focused source/profile contract also passed:

```sh
scripts/with-test-tmp.sh python3 scripts/agent-test-tests.py \
  AgentTestPlanTests.test_agent_test_window_profile_contract
```

This structural check is not the runtime oracle. No Rust suite, rb aggregate,
mutation run or cross-platform build ran in this recovery. The native build and
live observer are the evidence for the macOS-only behavior. The rb aggregate is
owed to integration; the coordinator reported rb unreachable. The private
WebKit selector is guarded and logs a warning if unavailable; this observation
proves support only on the macOS/WebKit installation used for this run.
