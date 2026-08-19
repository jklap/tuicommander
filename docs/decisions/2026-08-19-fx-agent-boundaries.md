# Integrate fx Through Its Native Agent Boundaries

## Status

Accepted by Boss on 2026-08-19.

## Problem

fx 0.0.3 is a native coding-agent harness with a persistent terminal UI,
one-shot headless execution, durable sessions, and an MCP client. Treating it
as a generic command would make it launchable but would leave TUICommander
unable to detect the binary, preserve a turn across restart, install the MCP
bridge in fx's accepted schema, or deliver an initial orchestrated prompt
without terminating the session.

The integration crosses the frontend agent registry, backend launch contract,
session persistence, and MCP client identity. Those surfaces must agree on one
agent type and on fx's native ownership rules.

## Decision

Add `fx` to the canonical `AgentType` registry and reuse TUICommander's existing
agent seams rather than introducing an fx-specific launch path.

- Interactive orchestration launches the bare `fx` TUI. The prompt placeholder
  is removed from argv and delivered through the existing deferred
  bracketed-paste-plus-Enter path because fx rejects positional prompts.
- Headless prompts use `fx ask --no-save "{prompt}"`; this mode is explicitly
  one-shot and does not back a persistent terminal session.
- Session binding is discovery-based. TUICommander reads recent
  `~/.fx/sessions/<id>/session.json` manifests under the launched process's
  `HOME`, validates the upstream session-ID grammar, manifest/directory ID
  equality, and normalized `workspace_root`, and resumes with
  `fx --resume <id>`.
- Resume keeps the run configuration's command and environment but does not
  append its launch arguments. fx 0.0.3 accepts only `--record` after
  `--resume`; a launch-time `--model` suffix makes restore exit with usage code
  1, while the native session already owns its model selection.
- MCP installation writes fx's native `~/.fx/mcp.json` entry under `mcp`, using
  a local server with an argv-vector command and explicit `enabled` and
  `required` booleans. The MCP client name `fx` maps back to the same agent type.
- Ask-mode permission overlays reuse the shared numbered `ChoicePrompt` wire
  contract. An optional `requires_confirmation` flag distinguishes fx's
  select-then-Enter interaction from agents whose numeric choice submits
  immediately; consumers that do not receive the field retain the existing
  immediate-submit behavior.
- fx has no TUIC-managed lifecycle-hook installer. Screen-state classification
  is enabled only after a real native PTY capture proves stable ready and
  working markers; source inspection alone is insufficient evidence.

## Why Existing Mechanisms Are Insufficient

Passing the prompt positionally makes fx exit with a CLI usage error. Reusing a
generic JSON MCP entry writes `type: "stdio"` and a string command, which is not
fx's native profile shape. Forced `TUIC_SESSION` injection would invent an ID
ownership contract that fx does not expose, while generic last-session resume
can bind a restored terminal to the wrong workspace. Declaring a ready-screen
adapter from documentation could falsely suspend a live turn because fx keeps
parts of its composer visible while work is active.

The generic run-config composer appends launch arguments after an agent's
resume flags. That is valid for existing agents but not fx: the official binary
rejects `fx --resume <id> --model <model>` before contacting its provider.

Treating fx's permission menu as an ordinary immediate-submit choice would
clear authoritative awaiting state on the number key and make the mobile client
omit the required Enter. Inferring confirmation from `dismiss_key` is also
incorrect because Escape dismissal and numeric submission are independent
parts of the interaction contract.

## Alternatives Considered

1. Launch `fx ask` for every orchestrated task. Rejected because it exits after
   one response and cannot support follow-ups, queued turns, or terminal resume.
2. Add an fx-only backend spawn route. Rejected because the existing prefill-only
   launch contract already expresses the required argv and input separation.
3. Bind fx sessions to `TUIC_SESSION`. Rejected because fx owns its session IDs
   and persists an authoritative workspace-scoped manifest.
4. Omit MCP auto-install and require manual setup. Rejected because fx exposes a
   stable native MCP profile format compatible with the existing installer.
5. Infer busy/idle markers from upstream source. Rejected until native PTY bytes
   demonstrate the rendered states and transition ordering.
6. Keep fx permissions on the generic silence-based question path. Rejected
   because the full-screen overlay has no shared prompt anchor, making the
   low-confidence awaiting state transient and losing structured options.
7. Reorder launch arguments before `--resume`. Rejected because fx's resume
   grammar does not accept the model override in either position; the saved
   session is the authority for its model.

## Trade-offs

Discovery performs a bounded directory and manifest scan on agent lifecycle
polls. The five-minute freshness window avoids adopting abandoned sessions but
means a delayed association is intentionally rejected. Following process
`HOME` preserves isolated run configurations, at the cost of platform-specific
process-environment reading already used by other agents. Deferring the screen
adapter can leave a persistent fx process conservatively busy until evidence is
available; that is safer than a false idle transition and auto-standby.

## Failure Semantics

Invalid IDs, symlinked session directories, oversized or malformed manifests,
ID mismatches, workspace mismatches, stale entries, and already-claimed IDs are
rejected. A restored terminal with a saved but unverifiable ID refuses automatic
resume; a terminal with no saved ID may use the explicit `fx --resume last`
fallback. Invalid MCP JSON is never overwritten by the repair pass. Prompt
delivery failure leaves the persistent process visible and follows the existing
pending-injection diagnostics rather than falling back to a one-shot invocation.
An fx permission selection remains awaiting after a numeric key until Enter is
sent; Escape and Tab retain the existing dismiss/amend clear semantics.
Unsupported launch-only arguments are dropped during fx resume rather than
turning a verified session into a deterministic CLI usage failure.

## Lifecycle and Ownership Boundaries

fx owns authentication, model selection, session contents, session IDs, and the
schemas of `settings.json`, `session.json`, and `mcp.json`. TUICommander owns
binary detection, process classification, safe prompt delivery, selection of a
verified resume operand, bridge entry installation/removal, and terminal
lifecycle state. The frontend renders the canonical registry; the Rust backend
owns filesystem validation and orchestration semantics.
