import { expect, test } from "vitest";
import { createTasksQuery } from "../../../stores/tasksQuery";
import type { WorkItem, WorkItemQuery } from "../../../types/work-items.gen";

const query: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 1,
};

function item(id: string, title: string): WorkItem {
	return {
		reference: { providerId: "local", accountId: "boss", id },
		key: id,
		title,
		itemType: "task",
		state: { id: "open", name: "Open", tone: "neutral" },
		assignees: [],
		labels: [],
		updatedAt: "2026-10-10T00:00:00Z",
		url: "",
		fields: {},
	};
}

// Catches: Refresh leaves cached later pages, presenting outdated task data as fresh.
test("refresh_then_load_more_never_reuses_outdated_later_page", async () => {
	let secondTitle = "Needs work";
	const provider = {
		providerId: "local",
		accountId: "boss",
		async list(request: WorkItemQuery) {
			return request.cursor === null
				? { items: [item("1", "First task")], nextCursor: "page-two" }
				: { items: [item("2", secondTitle)], nextCursor: null };
		},
		async grouped() {
			return { groups: [], nextCursor: null };
		},
	};
	const model = createTasksQuery();
	await model.select(provider, query);
	await model.loadMore();
	secondTitle = "Completed elsewhere";
	await model.refresh();
	await model.loadMore();
	expect(model.state().items.map((row) => row.title)).toEqual(["First task", "Completed elsewhere"]);
	expect(model.state().stale).toBe(false);
	model.dispose();
});
