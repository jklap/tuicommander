import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
const toastAdd = vi.fn();
const terminalSetActive = vi.fn();
const repoSetActive = vi.fn();
const workspaceSetActive = vi.fn();

vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		getPaths: () => ["/repo"],
		get: (path: string) => (path === "/repo" ? { displayName: "Repo" } : undefined),
		findOwnerForTerminal: () => ({ repoPath: "/repo", workspaceId: "main" }),
		setActive: repoSetActive,
		setActiveWorkspace: workspaceSetActive,
	},
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getTerminalForSession: (id: string) => (id === "live" ? "term-1" : null),
		setActive: terminalSetActive,
	},
}));

function status(snapshotCursor = 4, readCursor = 1) {
	return {
		projectRoot: "/repo",
		revision: snapshotCursor,
		snapshotCursor,
		readCursor,
		unreadCount: snapshotCursor - readCursor,
		collectionEnabled: true,
		workstreams: [],
		projectBlockers: [],
	};
}

function event(id = "event-1", sessionId = "live") {
	return {
		id,
		sequence: 4,
		revision: 4,
		createdAtMs: 1,
		type: "milestone" as const,
		summary: "Shipped safely",
		workstream: "Delivery",
		sessionId,
	};
}

describe("progressStore", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
	});

	it("freezes the opened watermark while a later refresh updates authoritative unread state", async () => {
		invokeMock.mockImplementation((command: string) =>
			Promise.resolve(command === "progress_status" ? status() : { revision: 4, snapshotCursor: 4, events: [event()] }),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		await store.refreshProject("/repo");
		expect(store.state.projects["/repo"].status?.snapshotCursor).toBe(4);

		invokeMock.mockImplementation((command: string) =>
			Promise.resolve(
				command === "progress_status" ? status(7, 1) : { revision: 7, snapshotCursor: 7, events: [event("later")] },
			),
		);
		await store.refreshProject("/repo", true);
		expect(store.state.projects["/repo"].status).toMatchObject({ snapshotCursor: 4, unreadCount: 6 });
	});

	it("presents one live toast by durable event id and opts out of MESSAGES mirroring", async () => {
		invokeMock.mockResolvedValue({
			projectRoot: "/repo",
			revision: 0,
			snapshotCursor: 0,
			readCursor: 0,
			unreadCount: 0,
			collectionEnabled: true,
			workstreams: [],
			projectBlockers: [],
		});
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		const payload = {
			repo_path: "/repo",
			payload: { receipt: { status: "recorded", revision: 4, eventId: "event-1" }, event: event() },
		};
		store.presentLive(payload);
		store.presentLive(payload);
		expect(toastAdd).toHaveBeenCalledTimes(1);
		expect(toastAdd.mock.calls[0][8]).toBe(false);
	});

	it("never navigates a closed provenance source to another terminal", async () => {
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		expect(store.openSource(event("closed", "gone"))).toBe(false);
		expect(terminalSetActive).not.toHaveBeenCalled();
		expect(store.openSource(event())).toBe(true);
		expect(terminalSetActive).toHaveBeenCalledWith("term-1");
	});

	it("pages backwards from the carried cursor, keeps the filter, and stops at the end of the list", async () => {
		invokeMock.mockImplementation((command: string) =>
			Promise.resolve(
				command === "progress_status"
					? status()
					: { revision: 4, snapshotCursor: 4, events: [event()], nextBeforeSequence: 4 },
			),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		await store.refreshProject("/repo", false, { blockerOnly: true });

		invokeMock.mockClear();
		invokeMock.mockResolvedValue({ revision: 4, snapshotCursor: 4, events: [event("older")] });
		await store.loadMore("/repo");

		// The page request carries the cursor AND the active filter. Dropping either
		// silently shows the first page again, or shows unfiltered rows below filtered ones.
		expect(invokeMock).toHaveBeenCalledTimes(1);
		expect(invokeMock).toHaveBeenCalledWith("progress_list", {
			project: "/repo",
			input: { blockerOnly: true, beforeSequence: 4, limit: 50 },
		});
		expect(store.state.projects["/repo"].events.map((e) => e.id)).toEqual(["event-1", "older"]);

		// A page with no further cursor is the end: the next call must not ask again.
		invokeMock.mockClear();
		await store.loadMore("/repo");
		expect(invokeMock).not.toHaveBeenCalled();
	});

	it("keeps command errors visible", async () => {
		invokeMock.mockRejectedValue(new Error("revision conflict"));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		expect(await store.clear("/repo", 3)).toBe(false);
		expect(store.state.projects["/repo"].error).toBe("revision conflict");
	});
});
