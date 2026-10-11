import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
});
