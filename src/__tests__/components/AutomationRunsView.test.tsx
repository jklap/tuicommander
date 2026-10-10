import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createRoot } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { focusAutomationRun } from "../../components/AutomationsDialog/RunsView";
import { createAutomationAdapter } from "../../components/AutomationsDialog/transportAdapter";
import { createAutomationsStore } from "../../stores/automations";
import { terminalsStore } from "../../stores/terminals";
import { rpc } from "../../transport";
import { getSessionConnection } from "../../transportRuntime";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { fakeAdapter, runFixture } from "../automationsFixtures";

vi.mock("../../utils/navigateToTerminal", () => ({ navigateToTerminal: vi.fn() }));
vi.mock("../../transportRuntime", async (original) => ({
	...(await original<typeof import("../../transportRuntime")>()),
	getSessionConnection: vi.fn(),
}));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc: vi.fn(),
}));

describe("automation runs", () => {
	it("keeps deleted automation snapshots visible and renders backend counts rather than counting the page", async () => {
		const history = vi.fn(async (_id?: string, offset = 0) =>
			offset
				? [runFixture("last", "deleted")]
				: Array.from({ length: 21 }, (_, i) => ({
						...runFixture(`r${i}`, "deleted"),
						status: (["needs_you", "unknown", "interrupted"] as const)[i % 3],
					})),
		);
		const runNow = vi.fn();
		render(() => (
			<AutomationsDialog
				onClose={() => {}}
				adapter={fakeAdapter({
					list: async () => [],
					history,
					runNow,
					summary: async (window) => ({
						total: window === "24h" ? 73 : 401,
						by_status: { needs_you: 11, unknown: 9, interrupted: 4 },
					}),
				})}
			/>
		));
		fireEvent.click(screen.getByRole("button", { name: "Runs" }));
		await screen.findByText("Total runs: 73");
		expect(screen.getByText("Total runs: 401")).toBeTruthy();
		expect(screen.getAllByText("needs_you").length).toBeGreaterThan(0);
		expect(screen.getAllByText("unknown").length).toBeGreaterThan(0);
		expect(screen.getAllByText("interrupted").length).toBeGreaterThan(0);
		expect(screen.getAllByRole("button", { name: "View run" })).toHaveLength(20);
		fireEvent.click(screen.getAllByRole("button", { name: "View run" })[0]);
		await screen.findByText("Live session unavailable. No replacement run was started.");
		expect(screen.getByText("No workspace was recorded.")).toBeTruthy();
		expect(runNow).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Next runs" }));
		await waitFor(() => expect(history).toHaveBeenCalledWith(undefined, 20, 21));
		await waitFor(() => expect(screen.getAllByRole("button", { name: "View run" })).toHaveLength(1));
		expect(screen.getByRole("button", { name: "Next runs" })).toBeDisabled();
	});
	it("does not lose saved stdout stderr or truncation evidence when the live session is missing", async () => {
		const run = {
			...runFixture("saved", "deleted"),
			workspace: "/repo/removed-worktree",
			stdout: { text: "Saved report", truncated: true },
			stderr: { text: "Saved error", truncated: false },
		};
		render(() => <AutomationsDialog onClose={() => {}} adapter={fakeAdapter({ history: async () => [run] })} />);
		fireEvent.click(screen.getByRole("button", { name: "Runs" }));
		fireEvent.click(await screen.findByRole("button", { name: "View run" }));
		expect(screen.getByText("Saved report")).toBeTruthy();
		expect(screen.getByText("Saved error")).toBeTruthy();
		expect(screen.getByText("/repo/removed-worktree")).toBeTruthy();
		expect(screen.getByText("Saved output was truncated by the backend.")).toBeTruthy();
	});
	it("never focuses a same-id session owned by another daemon or a suspended or exited terminal", () => {
		const run = { ...runFixture("remote", "a"), session_id: "session" };
		vi.spyOn(terminalsStore, "getTerminalForSession").mockReturnValue("term");
		const get = vi
			.spyOn(terminalsStore, "get")
			.mockReturnValue({ suspended: false, shellState: "idle" } as ReturnType<typeof terminalsStore.get>);
		vi.mocked(getSessionConnection).mockReturnValue("remote-owner");
		vi.mocked(navigateToTerminal).mockClear();
		expect(focusAutomationRun(run)).toBe(false);
		expect(focusAutomationRun(run, "remote-owner")).toBe(true);
		expect(navigateToTerminal).toHaveBeenCalledWith("term");
		get.mockReturnValue({ suspended: true, shellState: "idle" } as ReturnType<typeof terminalsStore.get>);
		expect(focusAutomationRun(run, "remote-owner")).toBe(false);
		get.mockReturnValue({ suspended: false, shellState: "exited" } as ReturnType<typeof terminalsStore.get>);
		expect(focusAutomationRun(run, "remote-owner")).toBe(false);
		vi.restoreAllMocks();
	});
	it("addresses remote history and summaries explicitly rather than reading the local ledger", async () => {
		const adapter = createAutomationAdapter("remote-owner");
		await adapter.history(undefined, 20, 21);
		await adapter.summary("7d");
		expect(rpc).toHaveBeenCalledWith(
			"automation_action",
			{ input: { action: "list_runs", id: undefined, offset: 20, limit: 21 } },
			"remote-owner",
		);
		expect(rpc).toHaveBeenCalledWith(
			"automation_action",
			{ input: { action: "summary", window: "7d" } },
			"remote-owner",
		);
	});
	it("ignores a stale history page so late previous requests cannot replace the requested page", async () => {
		await createRoot(async (dispose) => {
			let resolve!: (runs: ReturnType<typeof runFixture>[]) => void;
			const store = createAutomationsStore(
				fakeAdapter({
					history: async (_id, offset) =>
						offset === 0
							? new Promise((r) => {
									resolve = r;
								})
							: [runFixture("new", "a")],
				}),
			);
			const older = store.loadRuns();
			await store.loadRuns(20);
			resolve([runFixture("old", "a")]);
			await older;
			expect(store.allRuns()[0].id).toBe("new");
			expect(store.runsOffset()).toBe(20);
			dispose();
		});
	});
});
