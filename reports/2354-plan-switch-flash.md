# Plan selection flash — 2354-a8a3

## Cause and fix

A newly keyed `StoryNotes` component creates a pending notes resource. Without a local Suspense boundary, it suspends the host's lazy-loading boundary and removes the entire Plans and Stories dialog. The local notes boundary keeps the dialog in place and preserves per-story draft isolation.

The pre-fix commit was `922ee6c9d7a40c50930fead1dc876506126fbff0`. Three deferred-resource component cases reproduce dialog removal on plan, story and filter selection. The fix is `5c3ea3a92cddb1e6c5d0b39f00850a10bf34bf28`. Four focused Vitest files pass all 14 tests. Logs: `~/Gits/.tmp/2354-red.log` and `~/Gits/.tmp/2354-green-final.log`.

## Visible browser verification

The first native agent-test window was hidden. Its black screenshot is not evidence of rendering. The coordinator then authorized worktree Vite browser verification on a separate port.

- Frontend: this worktree's Vite server at `http://127.0.0.1:19855/`, not the daily Vite server.
- Backend: isolated `agent-test-2354`, TCP port `19854`. A temporary Vite middleware forwarded HTTP API calls to that instance's Unix socket. It did not use credentials, the daily backend, or mocked story replies.
- Browser: mandatory stealth wrapper, persistent `AB_SESSION=tuic2354-visible`, with a separate empty orphan-scan scope.
- Fixture: real persisted plans A and B, each with a story and criterion, in a throwaway repository.
- Sequence: A → B → A → B → A → B → A.
- Result: six completed selections, each with the expected story heading, the original dialog node and visible dialog ancestry.
- Continuous evidence: 770 animation-frame samples, all with the original visible dialog node. A MutationObserver watched subtree removals and visibility-related attributes on the dialog ancestry. It recorded no removal or visibility change. The document reported `visible`.

Evidence directory: `~/Gits/.tmp/2354-browser/`:

| File | Evidence |
| --- | --- |
| `mutation-log.json` | Six steps, 770 frame samples, no removal/hide events, `pass: true` |
| `00-plan-A.png` through `06-plan-A.png` | Seven browser-tab captures, alternating A and B |
| `01-state.json` through `06-state.json` | Selected story, node identity and visibility per step |
| `transcript.txt` | Wrapper commands, responses and exit codes |
| `run.py` | Capture procedure and independent identity/visibility assertions |
| `pixel-check.json` | Each screenshot has 536 colors; no all-black image |

The screenshots are valid 1000 × 1656 PNGs. Pixel checks reject a solid-color capture; they are not a visual design review. The DOM/frame log is the evidence for no removal or hiding between captures.

The temporary HTTP proxy did not bridge terminal WebSockets, so the unrelated terminal canvas reported an initial-frame timeout. The Plans and Stories HTTP calls completed against the isolated backend. No terminal-rendering claim is made.

The named browser tab and daemon, Vite server, and agent-test instance were stopped after capture. No production code changed during this follow-up check.
