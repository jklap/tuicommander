/**
 * Characterization tests for CanvasTerminal's pending-resume keydown handling
 * (`hasPendingResume`/`onResume`/`onResumeDismiss`/`pendingResumeIsClickOnly`
 * props) — the resume-banner key handling had zero test coverage before this.
 *
 * Two banner flavors, both exercised below:
 * - Restore-sourced (`pendingResumeIsClickOnly` omitted/false): Space/Enter
 *   accept, a printable key dismisses-and-passes-through, and
 *   Escape/Backspace/Delete/Tab dismiss without passing through.
 * - Exit-sourced (`pendingResumeIsClickOnly: true`): click-only — every key,
 *   including Space/Enter/Escape, skips this branch entirely and passes
 *   straight through to normal terminal input handling; the banner has no
 *   keyboard interaction at all.
 *
 * Keydown is dispatched on the hidden keyboard-capture `<input>`
 * (`keyInputRef`) — that's where CanvasTerminal's own listener is bound
 * (`bindings.listen(keyInputRef, "keydown", ...)`), not the `<canvas>`.
 */
import { fireEvent, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "../../components/Terminal/__tests__/helpers/mountCanvasTerminal";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>();
	return { ...actual, createTransport: () => fakeTransport.current! };
});

vi.mock("../../components/Terminal/glyphCache", () => ({
	getSharedMetrics: () => FIXED_CELL_METRICS,
	acquireCache: vi.fn(),
	releaseCache: vi.fn(),
	invalidateGlyphCache: vi.fn(),
}));

const TERM_ID = "resume-keydown-t1";
const SESSION_ID = "resume-keydown-s1";

let restoreCanvasEnv: () => void;

beforeEach(() => {
	restoreCanvasEnv = stubCanvasEnvironment();
	fakeTransport.current = createFakeTransport();
	terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
});

afterEach(() => {
	restoreCanvasEnv();
	terminalsStore.remove(TERM_ID);
});

async function mountWithResume(hasPendingResume: boolean, pendingResumeIsClickOnly = false) {
	const onResume = vi.fn();
	const onResumeDismiss = vi.fn();
	const mounted = await mountCanvasTerminal({
		sessionId: SESSION_ID,
		terminalId: TERM_ID,
		hasPendingResume,
		pendingResumeIsClickOnly,
		onResume,
		onResumeDismiss,
	});
	const input = mounted.container.querySelector("input");
	if (!input) throw new Error("hidden keyboard-input element not found");
	return { ...mounted, input, onResume, onResumeDismiss };
}

function writePtyCallCount(): number {
	return fakeTransport.current!.invokeCalls.filter((c) => c.cmd === "write_pty").length;
}

describe("CanvasTerminal resume keydown handling", () => {
	it("Space key calls onResume and does not write to the PTY", async () => {
		const { input, onResume, onResumeDismiss, dispose } = await mountWithResume(true);
		fireEvent.keyDown(input, { key: " " });
		await waitFor(() => expect(onResume).toHaveBeenCalledOnce());
		expect(onResumeDismiss).not.toHaveBeenCalled();
		expect(writePtyCallCount()).toBe(0);
		await dispose();
	});

	it("Enter key calls onResume and does not write to the PTY", async () => {
		const { input, onResume, onResumeDismiss, dispose } = await mountWithResume(true);
		fireEvent.keyDown(input, { key: "Enter" });
		await waitFor(() => expect(onResume).toHaveBeenCalledOnce());
		expect(onResumeDismiss).not.toHaveBeenCalled();
		expect(writePtyCallCount()).toBe(0);
		await dispose();
	});

	it("a printable character dismisses AND writes the key through to the PTY", async () => {
		const { input, onResume, onResumeDismiss, dispose } = await mountWithResume(true);
		fireEvent.keyDown(input, { key: "c" });
		await waitFor(() => expect(onResumeDismiss).toHaveBeenCalledOnce());
		expect(onResume).not.toHaveBeenCalled();
		await waitFor(() => expect(writePtyCallCount()).toBe(1));
		await dispose();
	});

	for (const key of ["Escape", "Backspace", "Delete", "Tab"]) {
		it(`${key} key dismisses and does NOT write to the PTY`, async () => {
			const { input, onResume, onResumeDismiss, dispose } = await mountWithResume(true);
			fireEvent.keyDown(input, { key });
			await waitFor(() => expect(onResumeDismiss).toHaveBeenCalledOnce());
			expect(onResume).not.toHaveBeenCalled();
			expect(writePtyCallCount()).toBe(0);
			await dispose();
		});
	}

	it("does nothing resume-specific when hasPendingResume is false", async () => {
		const { input, onResume, onResumeDismiss, dispose } = await mountWithResume(false);
		fireEvent.keyDown(input, { key: " " });
		expect(onResume).not.toHaveBeenCalled();
		expect(onResumeDismiss).not.toHaveBeenCalled();
		await dispose();
	});

	it("a multi-character key like ArrowLeft triggers neither accept nor dismiss", async () => {
		const { input, onResume, onResumeDismiss, dispose } = await mountWithResume(true);
		fireEvent.keyDown(input, { key: "ArrowLeft" });
		expect(onResume).not.toHaveBeenCalled();
		expect(onResumeDismiss).not.toHaveBeenCalled();
		await dispose();
	});

	describe("pendingResumeIsClickOnly (exit-sourced banner)", () => {
		async function mountClickOnly() {
			return await mountWithResume(true, true);
		}

		it("Space does NOT call onResume — passes through to the PTY instead", async () => {
			const { input, onResume, onResumeDismiss, dispose } = await mountClickOnly();
			fireEvent.keyDown(input, { key: " " });
			expect(onResume).not.toHaveBeenCalled();
			expect(onResumeDismiss).not.toHaveBeenCalled();
			await waitFor(() => expect(writePtyCallCount()).toBe(1));
			await dispose();
		});

		it("Enter does NOT call onResume — passes through to the PTY instead", async () => {
			const { input, onResume, onResumeDismiss, dispose } = await mountClickOnly();
			fireEvent.keyDown(input, { key: "Enter" });
			expect(onResume).not.toHaveBeenCalled();
			expect(onResumeDismiss).not.toHaveBeenCalled();
			await dispose();
		});

		it("a printable character passes through WITHOUT calling onResumeDismiss", async () => {
			const { input, onResume, onResumeDismiss, dispose } = await mountClickOnly();
			fireEvent.keyDown(input, { key: "c" });
			expect(onResumeDismiss).not.toHaveBeenCalled();
			expect(onResume).not.toHaveBeenCalled();
			await waitFor(() => expect(writePtyCallCount()).toBe(1));
			await dispose();
		});

		for (const key of ["Escape", "Backspace", "Delete", "Tab"]) {
			it(`${key} does NOT dismiss — the banner stays up`, async () => {
				const { input, onResume, onResumeDismiss, dispose } = await mountClickOnly();
				fireEvent.keyDown(input, { key });
				expect(onResumeDismiss).not.toHaveBeenCalled();
				expect(onResume).not.toHaveBeenCalled();
				await dispose();
			});
		}
	});
});
