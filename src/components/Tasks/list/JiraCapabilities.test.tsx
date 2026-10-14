import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, it } from "vitest";
import recorded from "../../../../plugins/jira/fixtures/grouped-priority-model.json";
// @ts-expect-error The provider is plain JavaScript.
import { createJiraProvider } from "../../../../plugins/jira/provider.js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { TasksList } from "./TasksList";

afterEach(cleanup);

// Catches: unsupported Jira grouping is offered and silently groups the recorded priorities by status.
it("recorded_jira_priority_rows_do_not_offer_unsupported_group_fields", async () => {
	const provider = createJiraProvider({
		account: { id: "review", host: "https://jira.example.invalid", kind: "basic" },
		fetch: async () => {
			throw new Error("This recorded model must not make HTTP requests");
		},
	}) as WorkItemProvider;
	const items = recorded.groups.flatMap((group) => group.items);
	provider.list = async () => ({ items: items as never[], nextCursor: null });
	// The captured list model has no create metadata; this test exercises group declarations only.
	provider.createFields = async () => [];
	render(() => <TasksList providers={[provider]} onOpen={() => {}} />);
	await screen.findByText(`${items.length} items`);
	fireEvent.click(screen.getByRole("button", { name: "Filters" }));
	const options = [...screen.getByLabelText("Group tasks").querySelectorAll("option")].map((option) => option.value);
	expect(options).toEqual(["", "status", "assignee", "issuetype"]);
	for (const unsupported of ["project", "labels", "priority"]) expect(options).not.toContain(unsupported);
});
