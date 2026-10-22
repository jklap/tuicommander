import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import recorded from "../../../../plugins/jira/fixtures/recorded-duedate.json";
// @ts-expect-error The Jira plugin is plain JavaScript without declarations.
import { createJiraProvider } from "../../../../plugins/jira/provider.js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { TasksRoadmap } from "./TasksRoadmap";

beforeEach(() => {
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1000);
});
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

// Recorded Jira Cloud search pages (scrubbed), served through the real Jira provider.
function jiraProvider() {
	return createJiraProvider({
		account: { id: "site", host: "https://jira.example.invalid", kind: "basic" },
		fetch: async (req: { body?: { nextPageToken?: string } }) => ({
			status: 200,
			headers: {},
			json: structuredClone(req.body?.nextPageToken ? recorded.pages[1] : recorded.pages[0]),
		}),
	}) as WorkItemProvider;
}

describe("Jira due dates on the Tasks roadmap", () => {
	// Catches: a real Jira due date dropped because the provider declares no date descriptor.
	it("due_date_shows_as_a_dated_milestone_and_undated_items_stay_listed", async () => {
		render(() => <TasksRoadmap provider={jiraProvider()} initialDate={new Date(2026, 9, 1)} onOpen={() => {}} />);
		await screen.findByText(/3 loaded/);
		expect(screen.getByRole("status")).toHaveTextContent("3 loaded · 2 dated · 1 without usable dates");
		const timeline = within(screen.getByRole("list", { name: "Dated tasks" }));
		expect(timeline.getByRole("button", { name: /LAN-21575/ })).toHaveTextContent("2026-10-16 – 2026-10-16");
		expect(timeline.getByRole("button", { name: /LAN-22579/ })).toHaveTextContent("2026-10-04 – 2026-10-04");
		const side = within(screen.getByRole("complementary", { name: "Tasks without usable dates" }));
		expect(side.getByRole("button", { name: /LAN-23350/ })).toBeInTheDocument();
		expect(screen.queryByText(/Start: Due date/)).not.toBeInTheDocument();
	});

	// Catches: the mapping applied to page one only, or lost when the list is refreshed.
	it("due_dates_survive_paging_and_refresh", async () => {
		render(() => <TasksRoadmap provider={jiraProvider()} initialDate={new Date(2026, 9, 1)} onOpen={() => {}} />);
		await screen.findByText(/3 loaded/);
		fireEvent.click(screen.getByRole("button", { name: "Load more" }));
		await screen.findByText(/4 loaded/);
		expect(screen.getByRole("status")).toHaveTextContent("4 loaded · 3 dated · 1 without usable dates");
		fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
		await screen.findByText(/3 loaded · 2 dated/);
		expect(screen.getByRole("button", { name: /LAN-21575/ })).toBeInTheDocument();
	});
});
