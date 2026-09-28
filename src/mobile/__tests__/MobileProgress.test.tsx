import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProgressDialog } from "../../components/ProgressDialog";
import { progressStore } from "../../stores/progress";

const { invoke } = vi.hoisted(() => ({
	invoke: vi.fn(async (command: string, args?: Record<string, unknown>) => {
		if (command !== "progress_list") return {};
		const project = args?.project;
		const text = project === "/newer" ? "Later work is blocked." : "Earlier work shipped.";
		return {
			project,
			entries: [
				{
					id: project === "/newer" ? 2 : 1,
					project,
					createdAtMs: 100,
					type: project === "/newer" ? "blocked" : "done",
					text,
				},
			],
			ptyIds: [],
		};
	}),
}));
vi.mock("../../invoke", () => ({ invoke }));

beforeEach(() => {
	progressStore.resetForTests();
	invoke.mockClear();
});
afterEach(() => {
	cleanup();
	progressStore.resetForTests();
});

describe("mobile Progress", () => {
	it("opens the newest populated project and lets Boss read another project's entries", async () => {
		render(() => <ProgressDialog embedded projects={["/newer", "/older"]} />);
		await waitFor(() => expect(screen.getByText("Later work is blocked.")).toBeTruthy());
		const picker = screen.getByLabelText("Progress project") as HTMLSelectElement;
		expect(picker.value).toBe("/newer");
		fireEvent.change(picker, { target: { value: "/older" } });
		await waitFor(() => expect(screen.getByText("Earlier work shipped.")).toBeTruthy());
		expect(screen.queryByText("Later work is blocked.")).toBeNull();
	});

	it("explains the empty project list instead of claiming the journal is empty", () => {
		render(() => <ProgressDialog embedded projects={[]} />);
		expect(screen.getByText("No project with Progress entries is available.")).toBeTruthy();
		expect(screen.queryByText("No progress recorded for this project yet.")).toBeNull();
	});

	it("does not claim an empty journal while the project list is loading", () => {
		render(() => <ProgressDialog embedded projects={undefined} />);
		expect(screen.getByText("Loading Progress projects…")).toBeTruthy();
		expect(screen.queryByText("No progress recorded for this project yet.")).toBeNull();
	});

	it("shows project discovery failure instead of a false empty journal", () => {
		render(() => <ProgressDialog embedded projects={undefined} projectsError="Connection lost" />);
		expect(screen.getByText("Progress projects are unavailable.")).toBeTruthy();
		expect(screen.queryByText("No progress recorded for this project yet.")).toBeNull();
	});
});
