import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
const toastAdd = vi.fn();

vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		state: { activeRepoPath: "/repo" },
		getPaths: () => ["/repo", "/other", "/third"],
		get: (path: string) => (path === "/repo" ? { displayName: "Repo" } : undefined),
	},
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: { activeId: "tab-a", terminals: { "tab-a": { sessionId: "pty-a", repoPath: "/repo", name: "Agent A" } } },
	},
}));

type Kind = "done" | "blocked" | "intent";

function entry(
	id: number,
	createdAtMs: number,
	type: Kind = "done",
): {
	id: number;
	project: string;
	createdAtMs: number;
	type: Kind;
	text: string;
	step: string;
	ptyId?: string;
} {
	return { id, project: "/repo", createdAtMs, type, text: `entry ${id}`, step: "Delivery" };
}

function list(entries: ReturnType<typeof entry>[], lastViewedMs?: number) {
	return { project: "/repo", entries, lastViewedMs };
}

describe("progressStore", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
	});

	it("opens the active PTY and can switch to another PTY in the same repository", async () => {
		invokeMock.mockResolvedValue({ ...list([]), ptyIds: ["pty-a", "pty-b"] });
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open();
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: { blockedOnly: false, ptyId: "pty-a" },
			}),
		);
		store.selectPty("pty-b");
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: { blockedOnly: false, ptyId: "pty-b" },
			}),
		);
		expect(store.selectedPtyId()).toBe("pty-b");
	});

	it("does not show the previous PTY's entries while another PTY loads", async () => {
		invokeMock.mockResolvedValueOnce({ ...list([{ ...entry(1, 100), ptyId: "pty-a" }]), ptyIds: ["pty-a", "pty-b"] });
		let finishLoad!: (value: ReturnType<typeof list>) => void;
		invokeMock.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishLoad = resolve;
				}),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo");
		await vi.waitFor(() => expect(store.state.projects["/repo"].entries).toHaveLength(1));
		store.selectPty("pty-b");
		expect(store.state.projects["/repo"].entries).toEqual([]);
		finishLoad(list([]));
		await vi.waitFor(() => expect(store.state.projects["/repo"].loading).toBe(false));
	});

	it("marks every viewed scope when closing after switching PTYs", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo");
		store.selectPty("pty-b");
		store.selectPty(null);
		await store.close();
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "pty-a" });
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "pty-b" });
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo" });
	});

	it("uses each PTY's own divider when switching scopes", async () => {
		invokeMock.mockImplementation((_command: string, args: { input?: { ptyId?: string } }) =>
			Promise.resolve(list([], args.input?.ptyId === "pty-b" ? 50 : 150)),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo");
		await vi.waitFor(() => expect(store.state.projects["/repo"].dividerMs).toBe(150));
		store.selectPty("pty-b");
		await vi.waitFor(() => expect(store.state.projects["/repo"].dividerMs).toBe(50));
	});

	/// The list is redrawn whenever an entry arrives. If the divider followed the
	/// stored timestamp it would jump to the top under the reader's cursor and
	/// the "what is new" question would become unanswerable mid-read.
	it("freezes the divider while the dialog is open and moves it on close", async () => {
		invokeMock.mockImplementation((command: string) =>
			command === "progress_list" ? Promise.resolve(list([entry(2, 200), entry(1, 100)], 150)) : Promise.resolve({}),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.open("/repo");
		await vi.waitFor(() => expect(store.state.projects["/repo"].entries).toHaveLength(2));
		expect(store.state.projects["/repo"].dividerMs).toBe(150);

		// A later entry arrives and the backend has already moved its own mark —
		// the frozen one must not follow.
		invokeMock.mockImplementation((command: string) =>
			command === "progress_list"
				? Promise.resolve(list([entry(3, 300), entry(2, 200), entry(1, 100)], 300))
				: Promise.resolve({}),
		);
		await store.refreshProject("/repo");
		expect(store.state.projects["/repo"].dividerMs).toBe(150);

		await store.close();
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "pty-a" });
		expect(store.state.projects["/repo"].dividerMs).toBeUndefined();
	});

	/// The old panel fanned out over every registered repository on open. The
	/// dialog shows one project, so it asks one question.
	it("queries once for the project it shows, never once per registered repository", async () => {
		invokeMock.mockResolvedValue(list([entry(1, 100)]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.open();
		await vi.waitFor(() => expect(store.state.projects["/repo"].entries).toHaveLength(1));
		expect(invokeMock).toHaveBeenCalledTimes(1);
		expect(invokeMock).toHaveBeenCalledWith("progress_list", {
			project: "/repo",
			input: { blockedOnly: false, ptyId: "pty-a" },
		});
	});

	it("carries the blocked-only filter into the query", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.open("/repo");
		await vi.waitFor(() => expect(invokeMock).toHaveBeenCalled());
		invokeMock.mockClear();
		store.setBlockedOnly(true);
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: { blockedOnly: true, ptyId: "pty-a" },
			}),
		);
	});

	it("toasts a reported entry silently and stays quiet for a host-written intent", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.presentLive({ repo_path: "/repo", payload: { entry: entry(1, 100, "blocked") } });
		expect(toastAdd).toHaveBeenCalledTimes(1);
		// `sound` is the 4th argument and `mirrorToMessages` the 9th: a progress
		// entry is never an interruption and never leaves the app.
		expect(toastAdd.mock.calls[0][3]).toBe(false);
		expect(toastAdd.mock.calls[0][8]).toBe(false);
		// The title is the step alone. The repo goes in as `repoPath` (7th
		// argument) and the toast draws it as its own badge, so a title of
		// "<project> · <step>" would print the project twice and leave the step
		// squashed into what the badge did not take.
		expect(toastAdd.mock.calls[0][0]).toBe("Delivery");
		expect(toastAdd.mock.calls[0][6]).toBe("/repo");

		store.presentLive({ repo_path: "/repo", payload: { entry: entry(2, 200, "intent") } });
		expect(toastAdd).toHaveBeenCalledTimes(1);
	});

	it("counts what arrived while the dialog was closed and clears it on open", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.presentLive({ repo_path: "/repo", payload: { entry: entry(1, 100) } });
		store.presentLive({ repo_path: "/repo", payload: { entry: entry(2, 200, "intent") } });
		expect(store.unreadCount).toBe(2);

		store.open("/repo");
		expect(store.unreadCount).toBe(0);

		// Open on the same project, the list refreshes instead of counting.
		store.presentLive({ repo_path: "/repo", payload: { entry: { ...entry(3, 300), ptyId: "pty-a" } } });
		expect(store.unreadCount).toBe(0);
	});

	it("keeps a failed command visible as one line", async () => {
		invokeMock.mockRejectedValue(new Error("database is locked"));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		expect(await store.deleteEntries("/repo", [1])).toBe(false);
		expect(store.state.projects["/repo"].error).toBe("database is locked");
	});
});
