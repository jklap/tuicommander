# Native plans and stories

Native stories can be managed without starting a workflow. TUICommander stores plans, stories, criteria, dependencies, and revisions in its configuration directory. A plan names one project and a source document; its state is derived from its stories.

Use `tuic story '<JSON action>' --project /absolute/project` to call the story service. For example:

```sh
tuic story '{"action":"create_plan","title":"Release","source":"plans/release.md"}' --project /absolute/project
tuic story '{"action":"list_plans"}' --project /absolute/project
```

The result is JSON with a `type` and `value`. Supply the returned plan ID when creating a story:

```sh
tuic story '{"action":"create_story","input":{"planId":"PLAN_ID","title":"Implement","criteria":["Behavior verified"],"priority":1,"origin":{"type":"native"},"fileScope":[]}}' --project /absolute/project
```

Changes to an existing story take its current `revision` as `expected_revision`; a stale request is rejected. A manual `claim` also takes `--session-id` with a live PTY ID from the same project. Closing that tab releases its claim. A session-bound agent can check criteria and submit review only on its own claim; a user performs review and administrative transitions.

The same records are available through desktop IPC, authenticated HTTP, and the `story` MCP tool. Import, export, and external sync are not part of this feature.
