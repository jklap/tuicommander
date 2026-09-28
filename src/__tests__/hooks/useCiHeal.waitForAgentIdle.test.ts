import { afterEach, describe, expect, it, vi } from "vitest";
import { waitForAgentIdle } from "../../hooks/useCiHeal";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal, testInScope } from "../helpers/store";

describe("waitForAgentIdle", () => {
	afterEach(() => {
		for (const id of Object.keys(terminalsStore.state.terminals)) terminalsStore.remove(id);
		terminalsStore._testCancelPendingTimers();
		vi.useRealTimers();
	});

	it("resolves immediately when the shell is idle with no declared background work", async () => {
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal());
			terminalsStore.update(id, { shellState: "idle" });
			await expect(waitForAgentIdle(id, 1_000)).resolves.toBeUndefined();
		});
	});

	it("resolves immediately when awaitingInput is set, even while busy with declared background work", async () => {
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal());
			terminalsStore.update(id, { shellState: "busy", declaredBackgroundWork: true });
			terminalsStore.setAwaitingInput(id, "question");
			await expect(waitForAgentIdle(id, 1_000)).resolves.toBeUndefined();
		});
	});

	it("does NOT resolve immediately, and times out, when the shell is idle but has declared background work", async () => {
		vi.useFakeTimers();
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal());
			terminalsStore.update(id, { shellState: "idle", declaredBackgroundWork: true });

			const promise = waitForAgentIdle(id, 1_000);
			const settled = vi.fn();
			promise.then(settled, settled);

			await vi.advanceTimersByTimeAsync(500);
			expect(settled).not.toHaveBeenCalled();

			// Deadline is exactly 1000ms out; the tick AT 1000ms sees Date.now() === deadline
			// (not `>`), so it doesn't fire yet — advance past the NEXT tick (1500ms) instead.
			await vi.advanceTimersByTimeAsync(1_000);
			await expect(promise).rejects.toThrow("Timeout waiting for agent idle");
		});
	});

	it("resolves once declared background work clears while polling", async () => {
		vi.useFakeTimers();
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal());
			terminalsStore.update(id, { shellState: "idle", declaredBackgroundWork: true });

			const promise = waitForAgentIdle(id, 5_000);
			const settled = vi.fn();
			promise.then(settled, settled);

			await vi.advanceTimersByTimeAsync(500);
			expect(settled).not.toHaveBeenCalled();

			terminalsStore.update(id, { declaredBackgroundWork: false });
			await vi.advanceTimersByTimeAsync(500);

			await expect(promise).resolves.toBeUndefined();
		});
	});

	it("rejects once the terminal is removed while waiting", async () => {
		vi.useFakeTimers();
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal());
			terminalsStore.update(id, { shellState: "busy" });

			const promise = waitForAgentIdle(id, 5_000);
			const settled = vi.fn();
			promise.then(settled, settled);

			await vi.advanceTimersByTimeAsync(500);
			expect(settled).not.toHaveBeenCalled();

			terminalsStore.remove(id);
			await vi.advanceTimersByTimeAsync(500);

			await expect(promise).rejects.toThrow("Terminal no longer exists");
		});
	});
});
