import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProgressDialog } from "../../components/ProgressDialog";
import { progressStore } from "../../stores/progress";

const { close, deleteEntries, open, setBlockedOnly, projectState, defaults } = vi.hoisted(() => {
	const entry = (id: number, type: string, text: string, createdAtMs: number) => ({
		id,
		project: "/repo",
		createdAtMs,
		type,
		text,
		agentName: "Worker",
	});
	const defaults = () => ({
		entries: [
			entry(3, "blocked", "Cannot reach the registry.", 300),
			entry(2, "intent", "Rewriting the dialog.", 200),
			entry(1, "done", "Shipped the store.", 100),
		],
		// Everything at or below this timestamp was on screen last visit.
		dividerMs: 200,
		loading: false,
		error: "one line for the whole failure",
	});
	return {
		close: vi.fn(),
		deleteEntries: vi.fn(),
		open: vi.fn(),
		setBlockedOnly: vi.fn(),
		projectState: defaults() as ReturnType<typeof defaults>,
		defaults,
	};
});

vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { get: () => ({ displayName: "Example" }) },
}));

vi.mock("../../stores/progress", () => ({
	progressStore: {
		requestedProject: () => "/repo",
		blockedOnly: () => false,
		setBlockedOnly,
		open,
		close,
		deleteEntries,
		state: { projects: { "/repo": projectState } },
	},
}));

beforeEach(() => {
	Object.assign(projectState, defaults());
	vi.clearAllMocks();
});

describe("ProgressDialog", () => {
	it("renders one newest-first list with the last-visit divider above the first old entry", () => {
		render(() => <ProgressDialog />);

		const texts = screen.getAllByText(/registry|dialog|store/).map((node) => node.textContent);
		expect(texts).toEqual(["Cannot reach the registry.", "Rewriting the dialog.", "Shipped the store."]);

		// dividerMs is 200, so entry 2 is the first one already seen.
		const rows = Array.from(document.querySelectorAll("[data-kind], [class*='divider']"));
		expect(rows.map((row) => row.getAttribute("data-kind") ?? "divider")).toEqual([
			"blocked",
			"divider",
			"intent",
			"done",
		]);
	});

	it("labels an intent as an intention rather than an outcome", () => {
		render(() => <ProgressDialog />);

		const intent = document.querySelector('[data-kind="intent"]');
		expect(intent?.textContent).toContain("set out to");
		expect(document.querySelector('[data-kind="done"]')?.textContent).not.toContain("set out to");
	});

	it("reads an unreadable journal as unavailable, never as an empty or finished project", () => {
		projectState.entries = [];

		render(() => <ProgressDialog />);

		expect(screen.getByText("This project's journal is unavailable.")).toBeTruthy();
		expect(screen.queryByText(/No progress recorded/)).toBeNull();
		expect(screen.queryByText(/Nothing is blocked/)).toBeNull();
	});

	it("says nothing was recorded only when the read succeeded", () => {
		projectState.entries = [];
		projectState.error = null as unknown as string;

		render(() => <ProgressDialog />);

		expect(screen.getByText("No progress recorded for this project yet.")).toBeTruthy();
	});

	it("shows one aggregated failure line and keeps the entries visible", () => {
		render(() => <ProgressDialog />);

		expect(screen.getAllByText("one line for the whole failure")).toHaveLength(1);
		expect(screen.getAllByText("Shipped the store.")).toHaveLength(1);
	});

	it("deletes the entry the button belongs to", () => {
		render(() => <ProgressDialog />);

		fireEvent.click(screen.getAllByLabelText("Delete entry")[1]);
		expect(deleteEntries).toHaveBeenCalledWith("/repo", [2]);
	});

	it("marks the visit as ended when the backdrop is clicked", () => {
		const { container } = render(() => <ProgressDialog />);

		// The overlay is the dialog's parent; the dialog itself stops the event.
		fireEvent.click(container.firstElementChild as HTMLElement);
		expect(close).toHaveBeenCalled();
	});

	it("embedded drops the overlay and asks the store to open the active project", () => {
		close.mockClear();
		const { container } = render(() => <ProgressDialog embedded />);

		expect(container.querySelector("[role='region']")).not.toBeNull();
		expect(screen.queryByLabelText("Close")).toBeNull();
		fireEvent.click(container.firstElementChild as HTMLElement);
		expect(close).not.toHaveBeenCalled();
	});

	it("offers exactly one blocked-only toggle", () => {
		render(() => <ProgressDialog />);

		const toggles = document.querySelectorAll("input[type='checkbox']");
		expect(toggles).toHaveLength(1);
		fireEvent.click(toggles[0]);
		expect(setBlockedOnly).toHaveBeenCalledWith(true);
	});
});

describe("ProgressDialog wiring", () => {
	it("uses the store the app exports", () => {
		expect(progressStore.requestedProject()).toBe("/repo");
	});
});
