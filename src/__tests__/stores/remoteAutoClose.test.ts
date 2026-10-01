import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScope } from "../helpers/store";

describe("remoteAutoClose", () => {
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let scheduleRemoteAutoClose: typeof import("../../stores/remoteAutoClose").scheduleRemoteAutoClose;
	let resumeStrandedAutoCloseTabs: typeof import("../../stores/remoteAutoClose").resumeStrandedAutoCloseTabs;
	let AGENT_TAB_AUTOCLOSE_MS: number;
	let REMOTE_TAB_AUTOCLOSE_MS: number;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		localStorage.clear();
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		const mod = await import("../../stores/remoteAutoClose");
		scheduleRemoteAutoClose = mod.scheduleRemoteAutoClose;
		resumeStrandedAutoCloseTabs = mod.resumeStrandedAutoCloseTabs;
		AGENT_TAB_AUTOCLOSE_MS = mod.AGENT_TAB_AUTOCLOSE_MS;
		REMOTE_TAB_AUTOCLOSE_MS = mod.REMOTE_TAB_AUTOCLOSE_MS;
	});

	afterEach(() => {
		terminalsStore._testCancelPendingTimers();
		vi.useRealTimers();
	});

	describe("scheduleRemoteAutoClose", () => {
		it("no-ops for a non-remote terminal, even once exited", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Local" }), isRemote: false });
				terminalsStore.update(id, { shellState: "exited" });

				scheduleRemoteAutoClose(id);
				vi.advanceTimersByTime(60_000);

				expect(terminalsStore.get(id)?.name).toBe("Local");
				expect(terminalsStore.get(id)).toBeDefined();
			});
		});

		it("no-ops for a remote terminal that has not exited", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Remote" }), isRemote: true });

				scheduleRemoteAutoClose(id);
				vi.advanceTimersByTime(60_000);

				expect(terminalsStore.get(id)?.name).toBe("Remote");
				expect(terminalsStore.get(id)).toBeDefined();
			});
		});

		it("starts a countdown and removes an exited agent-type remote terminal after AGENT_TAB_AUTOCLOSE_MS", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Agent" }), isRemote: true, agentType: "claude" });
				terminalsStore.update(id, { shellState: "exited" });

				scheduleRemoteAutoClose(id);
				expect(terminalsStore.get(id)?.name).toBe(`Agent (${Math.round(AGENT_TAB_AUTOCLOSE_MS / 1000)}s)`);

				vi.advanceTimersByTime(AGENT_TAB_AUTOCLOSE_MS - 1);
				expect(terminalsStore.get(id)).toBeDefined();

				vi.advanceTimersByTime(2);
				expect(terminalsStore.get(id)).toBeUndefined();
			});
		});

		it("starts a countdown and removes an exited non-agent remote terminal after REMOTE_TAB_AUTOCLOSE_MS", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Shell" }), isRemote: true });
				terminalsStore.update(id, { shellState: "exited" });

				scheduleRemoteAutoClose(id);

				// Shorter agent duration must not remove a plain remote tab early.
				vi.advanceTimersByTime(AGENT_TAB_AUTOCLOSE_MS);
				expect(terminalsStore.get(id)).toBeDefined();

				vi.advanceTimersByTime(REMOTE_TAB_AUTOCLOSE_MS - AGENT_TAB_AUTOCLOSE_MS + 1);
				expect(terminalsStore.get(id)).toBeUndefined();
			});
		});

		it("is idempotent: a second call mid-countdown does not restart or extend the deadline", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Shell" }), isRemote: true });
				terminalsStore.update(id, { shellState: "exited" });

				scheduleRemoteAutoClose(id);
				vi.advanceTimersByTime(REMOTE_TAB_AUTOCLOSE_MS - 5_000);

				// Mid-flight re-call must be a no-op — if it restarted the timer, the
				// tab would still be alive 5s past the ORIGINAL deadline below.
				scheduleRemoteAutoClose(id);

				vi.advanceTimersByTime(5_001);
				expect(terminalsStore.get(id)).toBeUndefined();
			});
		});

		it("prefers an explicit agentTypeHint over the store's current agentType", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Agent" }), isRemote: true });
				terminalsStore.update(id, { shellState: "exited" });

				scheduleRemoteAutoClose(id, "claude");

				expect(terminalsStore.get(id)?.name).toBe(`Agent (${Math.round(AGENT_TAB_AUTOCLOSE_MS / 1000)}s)`);
			});
		});
	});

	describe("resumeStrandedAutoCloseTabs", () => {
		it("starts countdowns for every eligible stuck terminal in one sweep, leaving others untouched", () => {
			testInScope(() => {
				const stuckShell = terminalsStore.add({ ...makeTerminal({ name: "Stuck1" }), isRemote: true });
				terminalsStore.update(stuckShell, { shellState: "exited" });
				const stuckAgent = terminalsStore.add({
					...makeTerminal({ name: "Stuck2" }),
					isRemote: true,
					agentType: "claude",
				});
				terminalsStore.update(stuckAgent, { shellState: "exited" });
				const liveLocal = terminalsStore.add({ ...makeTerminal({ name: "Live" }), isRemote: false });
				const liveRemote = terminalsStore.add({ ...makeTerminal({ name: "LiveRemote" }), isRemote: true });

				resumeStrandedAutoCloseTabs();

				expect(terminalsStore.get(stuckShell)?.name).toBe(`Stuck1 (${Math.round(REMOTE_TAB_AUTOCLOSE_MS / 1000)}s)`);
				expect(terminalsStore.get(stuckAgent)?.name).toBe(`Stuck2 (${Math.round(AGENT_TAB_AUTOCLOSE_MS / 1000)}s)`);
				expect(terminalsStore.get(liveLocal)?.name).toBe("Live");
				expect(terminalsStore.get(liveRemote)?.name).toBe("LiveRemote");

				vi.advanceTimersByTime(AGENT_TAB_AUTOCLOSE_MS + 1);
				expect(terminalsStore.get(stuckAgent)).toBeUndefined();
				expect(terminalsStore.get(stuckShell)).toBeDefined();
				expect(terminalsStore.get(liveLocal)).toBeDefined();
				expect(terminalsStore.get(liveRemote)).toBeDefined();

				vi.advanceTimersByTime(REMOTE_TAB_AUTOCLOSE_MS - AGENT_TAB_AUTOCLOSE_MS);
				expect(terminalsStore.get(stuckShell)).toBeUndefined();
			});
		});

		it("does not double-schedule a terminal that is already counting down", () => {
			testInScope(() => {
				const id = terminalsStore.add({ ...makeTerminal({ name: "Shell" }), isRemote: true });
				terminalsStore.update(id, { shellState: "exited" });

				scheduleRemoteAutoClose(id);
				vi.advanceTimersByTime(REMOTE_TAB_AUTOCLOSE_MS - 5_000);

				// Sweeping mid-flight must not reset this terminal's deadline either.
				resumeStrandedAutoCloseTabs();

				vi.advanceTimersByTime(5_001);
				expect(terminalsStore.get(id)).toBeUndefined();
			});
		});
	});
});
