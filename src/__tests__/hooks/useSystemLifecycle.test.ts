import { afterEach, describe, expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));

vi.mock("../../transport", () => ({ isTauri: () => true }));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../stores/github", () => ({
	githubStore: { stopPolling: vi.fn() },
}));

vi.mock("../../stores/notifications", () => ({
	notificationsStore: { clearBadge: vi.fn() },
}));

// Plain state object — the hook only ever reads settingsStore.state.preventSleepWhenBusy,
// and only at effect-mount time for this test's purposes, so a non-reactive mock is enough.
const { settingsState } = vi.hoisted(() => ({ settingsState: { preventSleepWhenBusy: true } }));
vi.mock("../../stores/settings", () => ({
	settingsStore: { state: settingsState },
}));

import { useSystemLifecycle } from "../../hooks/useSystemLifecycle";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal, testInScope } from "../helpers/store";

describe("useSystemLifecycle — sleep inhibition", () => {
	afterEach(() => {
		for (const id of Object.keys(terminalsStore.state.terminals)) terminalsStore.remove(id);
		terminalsStore._testCancelPendingTimers();
		mockInvoke.mockClear();
		settingsState.preventSleepWhenBusy = true;
	});

	it("blocks sleep when a terminal has declared background work despite an idle shell", () => {
		const id = terminalsStore.add(makeTerminal());
		terminalsStore.update(id, { shellState: "idle", declaredBackgroundWork: true });

		testInScope(() => useSystemLifecycle());

		expect(mockInvoke).toHaveBeenCalledWith("block_sleep");
	});

	it("does not block sleep when every terminal is idle with no declared background work", () => {
		const id = terminalsStore.add(makeTerminal());
		terminalsStore.update(id, { shellState: "idle" });

		testInScope(() => useSystemLifecycle());

		expect(mockInvoke).not.toHaveBeenCalledWith("block_sleep");
	});

	it("blocks sleep for a plain raw-busy terminal too (baseline, unrelated to declared background work)", () => {
		const id = terminalsStore.add(makeTerminal());
		terminalsStore.update(id, { shellState: "busy" });

		testInScope(() => useSystemLifecycle());

		expect(mockInvoke).toHaveBeenCalledWith("block_sleep");
	});

	it("never blocks sleep when preventSleepWhenBusy is off, even with declared background work", () => {
		settingsState.preventSleepWhenBusy = false;
		const id = terminalsStore.add(makeTerminal());
		terminalsStore.update(id, { shellState: "idle", declaredBackgroundWork: true });

		testInScope(() => useSystemLifecycle());

		expect(mockInvoke).not.toHaveBeenCalledWith("block_sleep");
	});
});
