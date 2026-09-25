# Native plans and stories

Native stories are managed directly in TUICommander. Plans, stories, criteria, dependencies, and revisions are stored in its configuration directory. A plan names one project and a source document; its state is derived from its stories.

Open a project, then select **Plans and Stories** in the toolbar. Create a plan with a title and source document or link. Add stories with acceptance criteria, priority, and optional relative file paths. Select a story to inspect its criteria, dependencies, and status. To work without an agent terminal, choose **Start work**, check criteria as they are met, then **Submit for review**. Approve the story or request changes; blocking and "Won't fix" are explicit manual actions. A dependency can be added while a story is Ready or Backlog. The dialog works in both desktop and browser mode.

The close button receives keyboard focus when the dialog opens. If the running backend predates native stories, the dialog asks you to restart TUICommander to load the newer backend.

Before loading plans, the dialog checks whether the running backend has native story support. A missing capability prompts a restart; a story action error remains visible as that action's error.

"Won't fix" cancels a story without delivering its output. It never satisfies a dependency or releases a dependent story for work. The Rust service marks a story **abandoned** when it or any dependency path reaches a cancelled story; the dialog renders that label. A human can remove a direct cancelled prerequisite from a Backlog story; this changes that story's requirements and makes it Ready only when every remaining prerequisite is Done. A separate Blocked story is never automatically unblocked. A plan remains Active while an unfinished dependent remains. A nonempty plan becomes Done when every story is either Done or Won't fix; an empty plan stays Draft. The service supplies the cancellation count and **All cancelled** flag to the dialog.

Select **Run history** for the selected plan to inspect persisted workflow runs. A run shows its status, story and attempt counts, and ordered events. Use **Load more events** to continue beyond the first page. The timeline follows run change notifications while the dialog is open; **Refresh** reloads the list of runs. Manual story work does not require a run.

Use `tuic story '<JSON action>' --project /absolute/project` to call the story service. For example:

```sh
tuic story '{"action":"create_plan","title":"Release","source":"plans/release.md"}' --project /absolute/project
tuic story '{"action":"list_plans"}' --project /absolute/project
```

The result is JSON with a `type` and `value`. Supply the returned plan ID when creating a story:

```sh
tuic story '{"action":"create_story","input":{"planId":"PLAN_ID","title":"Implement","criteria":["Behavior verified"],"priority":1,"origin":{"type":"native"},"fileScope":[]}}' --project /absolute/project
```

Changes to an existing story take its current `revision` as `expected_revision`; a stale request is rejected. The user-only `start_manual` transition moves a Ready story into progress without a PTY claim. A terminal `claim` instead takes `--session-id` with a live PTY ID from the same project. Closing that tab releases its claim. A session-bound agent can check criteria and submit review only on its own claim; a user performs review and administrative transitions. `transition_history` shows who performed each committed transition; approval from a managed MCP session is refused.

To remove a cancelled prerequisite, send `{"action":"remove_dependency","story_id":"DEPENDENT_ID","dependency_id":"CANCELLED_ID","expected_revision":2}` as a user action. Session-bound agent calls, stale revisions, non-Backlog dependents, and prerequisites other than Won't fix are rejected.

This follows the native workflow trust model: a managed MCP call carries a session identity and cannot remove a dependency. CLI and HTTP requests without a session identity are treated as user actions. This is a local workflow convention, not an authentication or security boundary; a local caller can omit a session identity.

The same records are available through desktop IPC, authenticated HTTP, and the `story` MCP tool. Import, export, and external sync are not part of this feature.
