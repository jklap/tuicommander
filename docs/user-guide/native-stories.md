# Native plans and stories

Native stories are managed directly in TUICommander. Plans, stories, criteria, dependencies, and revisions are stored in its configuration directory. A plan names one project and a source document; its state is derived from its stories.

Open a project, then select **Plans and Stories** in the toolbar. Choose **New plan** to see Markdown plans in the project's `plans/` and `.claude/plans/` directories. Select a plan; its title comes from its front-matter `title` or first Markdown heading. Files in nested directories such as `plans/archive/` are excluded. Choose **Refresh** after an agent creates a plan file. **Add from path or link** remains available for another document; a web link needs a typed title because its document is not read locally. Add stories with acceptance criteria, priority, and optional relative file paths. Select a story to inspect its criteria, dependencies, and status. To work without an agent terminal, choose **Start work**, check criteria as they are met, then **Submit for review**. Approve the story or request changes; blocking and "Won't fix" are explicit manual actions. A dependency can be added while a story is Ready or Backlog. The dialog works in both desktop and browser mode.

The close button receives keyboard focus when the dialog opens. If the running backend predates native stories, the dialog asks you to restart TUICommander to load the newer backend.

Before loading plans, the dialog checks whether the running backend has native story support. A missing capability prompts a restart; a story action error remains visible as that action's error.

"Won't fix" cancels a story without delivering its output. It never satisfies a dependency or releases a dependent story for work. The Rust service marks a story **abandoned** when it or any dependency path reaches a cancelled story; the dialog renders that label. A human can remove a direct cancelled prerequisite from a Backlog story; this changes that story's requirements and makes it Ready only when every remaining prerequisite is Done. A separate Blocked story is never automatically unblocked. A plan remains Active while an unfinished dependent remains. A nonempty plan becomes Done when every story is either Done or Won't fix; an empty plan stays Draft. The service supplies the cancellation count and **All cancelled** flag to the dialog.

Select **Run history** for the selected plan to inspect persisted workflow runs. A run shows its status, story and attempt counts, and ordered events. Use **Load more events** to continue beyond the first page. The timeline follows run change notifications while the dialog is open; **Refresh** reloads the list of runs. Manual story work does not require a run.

Use `tuic story '<JSON action>' --project /absolute/project` to call the story service. For example:

```sh
tuic story '{"action":"create_plan","title":"Release","source":"plans/release.md"}' --project /absolute/project
tuic story '{"action":"list_plans"}' --project /absolute/project
tuic story '{"action":"list_plan_sources"}' --project /absolute/project
tuic story '{"action":"add_plan_source","source":"plans/release.md"}' --project /absolute/project
```

The result is JSON with a `type` and `value`. Supply the returned plan ID when creating a story:

```sh
tuic story '{"action":"create_story","input":{"planId":"PLAN_ID","title":"Implement","criteria":["Behavior verified"],"priority":1,"origin":{"type":"native"},"fileScope":[]}}' --project /absolute/project
```

Changes to an existing story take its current `revision` as `expected_revision`; a stale request is rejected. The `start_manual` transition moves a Ready story into progress without a PTY claim. A terminal `claim` instead takes `--session-id` with a live PTY ID from the same project. Closing that tab releases its claim. A session-bound agent can perform the same transitions as other trusted callers. `transition_history` shows who performed each committed transition, including approval. Any trusted caller can approve after checking the acceptance criteria. Sessionless local HTTP records `local_api` provenance; a valid token records `human` provenance. These labels describe the transport, not a verified person. The desktop and browser dialogs both offer approval.

To remove a cancelled prerequisite, send `{"action":"remove_dependency","story_id":"DEPENDENT_ID","dependency_id":"CANCELLED_ID","expected_revision":2}` as a user action. Session-bound agent calls, stale revisions, non-Backlog dependents, and prerequisites other than Won't fix are rejected.

This follows the native workflow trust model: a managed MCP call carries a session identity and cannot remove a dependency. CLI and HTTP requests without a session identity are treated as user actions. This is a local workflow convention, not an authentication or security boundary; a local caller can omit a session identity.

The same records are available through desktop IPC, guarded HTTP, and the `story` MCP tool. `Done` records an approval with actor provenance. In a manual plan with no workflow run, approval immediately releases dependents whose other prerequisites are Done. Once a workflow run owns the plan, a dependent remains held until its accepted prerequisite has a current integration receipt: the checked source commit must be merged into the canonical branch and TUIC must pass the published checks on the merge result. A later unrecorded ref or tree change invalidates that proof. Import, export, and external sync are not part of this feature.

## Which caller can do what

| Caller | Scope | Reads | Story actions (claim, check criteria, submit review, add dependency, create, approve) | Administrative actions (reject, block, unblock, won't fix, start manual, remove dependency) |
|---|---|---|---|---|
| `story` MCP tool in a managed session | The session's own registered project only | Yes | Yes | Yes |
| Plans and Stories dialog | The open project | Yes | Yes | Yes |
| `tuic story ... --project /abs/path` (HTTP `POST /stories/action?path=...`, loopback or authenticated) | Any project named by `--project` | Yes | Yes | Yes (actor provenance follows the session or credentials) |

An orchestrating session in another project cannot read or approve this repo's plan through its `story` MCP tool: the tool resolves the project from the calling session, so a plan id of another project is refused with `plan does not belong to project`. It can do both from a shell with `tuic story '{"action":"plan_view","plan_id":"..."}' --project /abs/path/of/the/repo` and, for review, `{"action":"transition","story_id":"...","expected_revision":N,"command":"approve"}`. That path carries no session identity, so it records local API provenance unless credentials are supplied; as stated above, this is a workflow convention, not a security boundary. No cross-project MCP path exists, and none was added.

## MCP tool schema

The `story` tool publishes the full `StoryAction` JSON Schema (actions, fields, types, enums, the `origin` tag shape, priority range 1 to 3) in its `inputSchema`. It is generated from the Rust types, so it follows the code. Inside `input` the story fields are camelCase (`planId`, `fileScope`) while the action fields are snake_case (`story_id`, `expected_revision`).

Actor identity is tracking only and never restricts an action. Localhost remains trusted, including local token exchange. Administrative story decisions use the same state and revision rules for human, local API and managed callers.

A workflow plan reaches Done after all stories are approved and their integration checks are current. Moving the canonical branch or committing new work returns the plan to Active until those checks are recertified. Manual plans continue to finish through approval.

The graph runtime is being delivered in stages. The first stage stores replayable graph positions and requires explicit pause destinations and final checks for executable definitions, but does not automatically run the Designer's graph. Existing pre-contract runs remain available for inspection and cancellation; resuming one requires starting a new run instead. New record-only runs retain the existing explicit command controls during rollout. Fork/all-Join settings and automatic execution are not yet Designer capabilities.

The daemon executor now owns recovery and duration timers under an OS run-database lock. Reads never recover live work, and a non-owner daemon refuses run mutations. Graph resume uses `resume_graph {execution_id,activation_id,resolution}` with an explicit pending activation; status-only resume cannot bypass graph position. Graph start controls, Agent effects and delivery policy remain unavailable until their later slices.
