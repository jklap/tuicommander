import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkItem } from "../../../types/work-items.gen";
import { allDemoRows, listDemo } from "./demo";
import { TaskRows } from "./TaskRows";
import { TasksList } from "./TasksList";

// happy-dom has no layout. Supply viewport geometry, keeping the real virtualizer.
beforeEach(() => {
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
});

afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.restoreAllMocks();
});

describe("Tasks list", () => {
	// Catches: a large provider result creates one mounted DOM row per item.
	it("list_renders_bounded_dom_for_10k_items", async () => {
		const items = await allDemoRows(listDemo(10_000));
		expect(items).toHaveLength(10_000);
		render(() => <TaskRows items={items} groups={[]} loading={false} resetKey="all" onOpen={() => {}} />);
		await waitFor(() => expect(screen.getAllByRole("listitem").length).toBeGreaterThan(0));
		expect(screen.getAllByRole("listitem").length).toBeLessThan(40);
		expect(screen.getByRole("button", { name: /Task 00001/ })).toBeInTheDocument();
	});

	// Catches: query controls change their labels but do not change real provider rows.
	it("provider_filter_group_and_sort_change_rendered_results", async () => {
		render(() => <TasksList providers={[listDemo(4)]} onOpen={() => {}} />);
		await screen.findByRole("button", { name: /Task 00004/ });
		fireEvent.click(screen.getByRole("button", { name: "Filters" }));
		fireEvent.change(screen.getByLabelText("Filter Priority"), { target: { value: "high" } });
		await waitFor(() => expect(screen.queryByRole("button", { name: /Task 00002/ })).not.toBeInTheDocument());
		expect(screen.getByRole("button", { name: /Task 00003/ })).toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("Group tasks"), { target: { value: "priority" } });
		await screen.findByRole("heading", { name: "High" });
		fireEvent.change(screen.getByLabelText("Sort tasks"), { target: { value: "title" } });
		fireEvent.click(screen.getByLabelText("Reverse task sort"));
		await waitFor(() => expect(screen.getAllByRole("listitem")[0]).toHaveTextContent("Task 00003"));
	});

	// Catches: every keystroke requests provider data instead of debouncing the search.
	it("search_waits_for_quiet_input_then_displays_matching_rows", async () => {
		render(() => <TasksList providers={[listDemo(4)]} onOpen={() => {}} />);
		await screen.findByRole("button", { name: /Task 00004/ });
		vi.useFakeTimers();
		const input = screen.getByLabelText("Search tasks");
		fireEvent.input(input, { target: { value: "00002" } });
		await vi.advanceTimersByTimeAsync(200);
		expect(screen.getAllByRole("listitem")).toHaveLength(4);
		fireEvent.input(input, { target: { value: "00003" } });
		await vi.advanceTimersByTimeAsync(249);
		expect(screen.getAllByRole("listitem")).toHaveLength(4);
		await vi.advanceTimersByTimeAsync(1);
		expect(screen.getAllByRole("listitem")).toHaveLength(1);
		expect(screen.getByRole("button", { name: /Task 00003/ })).toBeInTheDocument();
	});

	// Catches: source changes retain a filter or pending search from the previous account.
	it("source_switch_cancels_pending_search_and_opens_selected_account", async () => {
		const open = vi.fn();
		render(() => <TasksList providers={[listDemo(4, "a"), listDemo(2, "b")]} onOpen={open} />);
		await screen.findByRole("button", { name: /Task 00004/ });
		vi.useFakeTimers();
		fireEvent.input(screen.getByLabelText("Search tasks"), { target: { value: "no match" } });
		fireEvent.change(screen.getByLabelText("Task source"), { target: { value: JSON.stringify(["demo", "b"]) } });
		await vi.advanceTimersByTimeAsync(300);
		expect(screen.getByLabelText("Search tasks")).toHaveValue("");
		fireEvent.click(screen.getByRole("button", { name: /Task 00002/ }));
		expect(open).toHaveBeenCalledWith({ providerId: "demo", accountId: "b", id: "2" });
	});

	// Catches: filter inputs always open (they push every row off a narrow screen) or a count that ignores applied filters.
	it("filters_collapse_behind_a_button_that_counts_active_filters", async () => {
		render(() => <TasksList providers={[listDemo(4)]} onOpen={() => {}} />);
		await screen.findByRole("button", { name: /Task 00004/ });
		const toggle = screen.getByRole("button", { name: "Filters" });
		expect(toggle).toHaveAttribute("aria-expanded", "false");
		expect(screen.queryByLabelText("Filter Priority")).not.toBeInTheDocument();
		fireEvent.click(toggle);
		expect(toggle).toHaveAttribute("aria-expanded", "true");
		fireEvent.change(screen.getByLabelText("Filter Priority"), { target: { value: "high" } });
		fireEvent.change(screen.getByLabelText("Group tasks"), { target: { value: "priority" } });
		const counted = await screen.findByRole("button", { name: "Filters (2)" });
		fireEvent.click(counted);
		expect(screen.queryByLabelText("Filter Priority")).not.toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Filters (2)" })).toBeInTheDocument();
	});

	describe("row metadata and selection", () => {
		const row = (id: string, state: WorkItem["state"], assignees: string[], updatedAt: string): WorkItem => ({
			reference: { providerId: "demo", accountId: "local", id },
			key: `DEMO-${id}`,
			title: `Row ${id}`,
			itemType: "task",
			state,
			assignees: assignees.map((name) => ({ id: name, name, avatarUrl: null })),
			labels: [],
			updatedAt,
			url: "",
			fields: {},
		});
		const hoursAgo = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();
		const items = [
			row("1", { id: "open", name: "Open", tone: "info" }, ["Ada Lovelace"], hoursAgo(3)),
			row("2", { id: "merged", name: "Merged", tone: "success" }, ["Grace", "Linus"], hoursAgo(50)),
			row("3", { id: "closed", name: "Closed", tone: "danger" }, [], hoursAgo(0.5)),
		];
		const mount = (selected: WorkItem["reference"] | null) =>
			render(() => (
				<TaskRows items={items} groups={[]} loading={false} resetKey="r" selected={selected} onOpen={() => {}} />
			));

		// Catches: the open task is indistinguishable from its neighbours.
		it("marks only the open task row as current", async () => {
			mount(items[1].reference);
			await screen.findByRole("button", { name: /Row 2/ });
			expect(screen.getByRole("button", { name: /Row 2/ })).toHaveAttribute("aria-current", "true");
			expect(screen.getByRole("button", { name: /Row 1/ })).not.toHaveAttribute("aria-current");
			expect(screen.getByRole("button", { name: /Row 3/ })).not.toHaveAttribute("aria-current");
		});

		// Catches: assignee dropped, extra assignees hidden without a count, or an unassigned row showing a blank.
		it("shows the first assignee with a count for the rest, and a label when unassigned", async () => {
			mount(null);
			const first = await screen.findByRole("button", { name: /Row 1/ });
			expect(first).toHaveTextContent("Ada Lovelace");
			expect(screen.getByRole("button", { name: /Row 2/ })).toHaveTextContent("Grace +1");
			expect(screen.getByRole("button", { name: /Row 3/ })).toHaveTextContent("Unassigned");
		});

		// Catches: updated time shown as a raw ISO string or computed in the wrong unit.
		it("shows the updated time as a relative age", async () => {
			mount(null);
			await screen.findByRole("button", { name: /Row 1/ });
			expect(screen.getByRole("button", { name: /Row 1/ })).toHaveTextContent("3h");
			expect(screen.getByRole("button", { name: /Row 2/ })).toHaveTextContent("2d");
			expect(screen.getByRole("button", { name: /Row 3/ })).toHaveTextContent("30m");
		});

		// Catches: state conveyed by colour only; each tone must draw its own shape.
		it("draws a distinct shape per state tone, not colour alone", async () => {
			const tones: WorkItem["state"]["tone"][] = ["neutral", "info", "success", "warning", "danger"];
			const many = tones.map((tone, index) => row(String(index), { id: tone, name: tone, tone }, [], hoursAgo(1)));
			render(() => <TaskRows items={many} groups={[]} loading={false} resetKey="s" onOpen={() => {}} />);
			await screen.findAllByRole("listitem");
			const shapes = screen.getAllByRole("listitem").map((entry) => {
				const icon = entry.querySelector("svg[data-state-icon]");
				expect(icon).not.toBeNull();
				expect(icon).toHaveAttribute("aria-hidden", "true");
				return icon?.innerHTML;
			});
			expect(new Set(shapes).size).toBe(tones.length);
		});

		// Catches: the linked-worktree mark on every row, on none, or on the wrong row.
		it("marks only the rows whose task is linked to a worktree", async () => {
			render(() => (
				<TaskRows
					items={items}
					groups={[]}
					loading={false}
					resetKey="l"
					linkedBranch={(reference) => (reference.id === "2" ? "feat/row-2" : undefined)}
					onOpen={() => {}}
				/>
			));
			await screen.findByRole("button", { name: /Row 2/ });
			expect(screen.getByRole("img", { name: "Linked to worktree feat/row-2" })).toBeInTheDocument();
			expect(screen.getAllByRole("img", { name: /Linked to worktree/ })).toHaveLength(1);
			expect(screen.getByRole("button", { name: /Row 2/ })).toContainElement(
				screen.getByRole("img", { name: "Linked to worktree feat/row-2" }),
			);
		});
	});
});
