/**
 * Real-mount coverage for Terminal.tsx's event bridge — the layer that has
 * had zero direct tests despite being the exact convergence point for two
 * things this session's diagnosis and fixes both touched:
 *
 *  - the `"agent-block"` ParsedEvent case, which calls
 *    `terminalsStore.handleOsc133("A"/"D", ...)` (the hook-driven block
 *    source) — never exercised through the real component before this file.
 *  - the duplicate `pty-osc133-<sessionId>` listener `e63fc9ee` removed —
 *    this pins that there is now exactly ONE registration for that event
 *    name at the component-tree level (mounting the real `Terminal`, not
 *    just unit-testing the transport class in isolation, which is what
 *    `canvasTerminalTransport.test.ts` already does and would NOT catch a
 *    regression where some component re-adds a raw `listen()` call outside
 *    the transport abstraction).
 *
 * `CanvasTerminal` and `TerminalSearch` are stubbed — their own mount
 * behavior is covered elsewhere (`canvasTerminalGestures.pin.test.ts`,
 * `canvasTerminalScrollbarMarks.mount.test.ts`, etc.); this file is only
 * about what Terminal.tsx itself wires up around them.
 */

import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal } from "../../../__tests__/helpers/store";
import { terminalsStore } from "../../../stores/terminals";
import { Terminal } from "../Terminal";

type Handler = (event: { payload: unknown }) => void;

const listenHandlers = vi.hoisted(() => new Map<string, Handler[]>());
const listenCalls = vi.hoisted(() => [] as string[]);

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn((eventName: string, handler: Handler) => {
		listenCalls.push(eventName);
		const arr = listenHandlers.get(eventName) ?? [];
		arr.push(handler);
		listenHandlers.set(eventName, arr);
		return Promise.resolve(() => {
			const remaining = (listenHandlers.get(eventName) ?? []).filter((h) => h !== handler);
			listenHandlers.set(eventName, remaining);
		});
	}),
	emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../CanvasTerminal", () => ({
	default: (props: { onRef?: (ref: unknown) => void }) => {
		props.onRef?.({
			focus: vi.fn(),
			scrollToBlock: vi.fn(),
			toggleBlockFold: vi.fn(),
			getSelectionText: vi.fn(() => ""),
			refresh: vi.fn(),
			paste: vi.fn(),
			toggleSearchBlockScope: vi.fn(),
			toggleCompose: vi.fn(),
			openComposeWithText: vi.fn(),
			searchBuffer: vi.fn(() => Promise.resolve([])),
			scrollToLine: vi.fn(),
			scrollToTop: vi.fn(),
			scrollToBottom: vi.fn(),
			scrollPages: vi.fn(),
			getBufferLines: vi.fn(() => Promise.resolve([])),
			openSearch: vi.fn(),
		});
		return document.createElement("div");
	},
}));

vi.mock("../TerminalSearch", () => ({
	TerminalSearch: (props: { onRef?: (ref: unknown) => void }) => {
		props.onRef?.({ open: vi.fn(), close: vi.fn(), toggleBlockScope: vi.fn() });
		return document.createElement("div");
	},
}));

function fire(eventName: string, payload: unknown) {
	for (const handler of listenHandlers.get(eventName) ?? []) {
		handler({ payload });
	}
}

const TERMINAL_ID = "bridge-t1";
const SESSION_ID = "bridge-s1";

beforeEach(() => {
	listenHandlers.clear();
	listenCalls.length = 0;
	terminalsStore.register(TERMINAL_ID, makeTerminal({ sessionId: SESSION_ID }));
});

afterEach(() => {
	terminalsStore.remove(TERMINAL_ID);
});

describe("Terminal.tsx event bridge (real mount)", () => {
	it("registers zero pty-osc133 listeners itself (CanvasTerminal owns the one that remains)", async () => {
		// e63fc9ee removed Terminal.tsx's own pty-osc133-<sid> listener, which used
		// to double-dispatch alongside the one CanvasTerminal's transport layer
		// already owned. CanvasTerminal is stubbed out in this file (its own mount
		// behavior is covered elsewhere), so this specifically pins that
		// Terminal.tsx itself doesn't reintroduce a second one — combined with
		// canvasTerminalTransport.test.ts's "exactly one" test on the surviving
		// listener, the two together cover the full guarantee without needing a
		// single test to mount both real components at once.
		const { unmount } = render(() => Terminal({ id: TERMINAL_ID }));
		await waitFor(() => {
			if (!listenCalls.includes(`pty-parsed-${SESSION_ID}`)) throw new Error("not attached yet");
		});

		expect(listenCalls.filter((name) => name === `pty-osc133-${SESSION_ID}`)).toHaveLength(0);

		unmount();
	});

	it('routes a "agent-block" start event through handleOsc133("A")', async () => {
		const { unmount } = render(() => Terminal({ id: TERMINAL_ID }));
		await waitFor(() => {
			if (!listenCalls.includes(`pty-parsed-${SESSION_ID}`)) throw new Error("not attached yet");
		});

		fire(`pty-parsed-${SESSION_ID}`, {
			type: "agent-block",
			action: "start",
			line: 42,
			prompt_text: "fix the parser",
		});

		await waitFor(() => {
			expect(terminalsStore.get(TERMINAL_ID)?.activeBlock?.promptLine).toBe(42);
		});
		expect(terminalsStore.get(TERMINAL_ID)?.activeBlock?.promptText).toBe("fix the parser");

		unmount();
	});

	it('routes a "agent-block" end event through handleOsc133("D") with the real exit code', async () => {
		const { unmount } = render(() => Terminal({ id: TERMINAL_ID }));
		await waitFor(() => {
			if (!listenCalls.includes(`pty-parsed-${SESSION_ID}`)) throw new Error("not attached yet");
		});

		fire(`pty-parsed-${SESSION_ID}`, { type: "agent-block", action: "start", line: 10 });
		await waitFor(() => {
			expect(terminalsStore.get(TERMINAL_ID)?.activeBlock?.promptLine).toBe(10);
		});

		fire(`pty-parsed-${SESSION_ID}`, { type: "agent-block", action: "end", line: 20, exit_code: 1 });

		await waitFor(() => {
			const blocks = terminalsStore.get(TERMINAL_ID)?.commandBlocks ?? [];
			expect(blocks.length).toBe(1);
		});
		const block = terminalsStore.get(TERMINAL_ID)!.commandBlocks[0];
		expect(block.endLine).toBe(20);
		expect(block.exitCode).toBe(1);

		unmount();
	});

	it('an "agent-block" end event with no exit_code carries exitCode null, not 0', async () => {
		// Regression pin for the `?? undefined` -> `?? null` distinction: a clean
		// turn must not paint a false-successful (0) or false-failed gutter tick.
		const { unmount } = render(() => Terminal({ id: TERMINAL_ID }));
		await waitFor(() => {
			if (!listenCalls.includes(`pty-parsed-${SESSION_ID}`)) throw new Error("not attached yet");
		});

		fire(`pty-parsed-${SESSION_ID}`, { type: "agent-block", action: "start", line: 5 });
		await waitFor(() => {
			expect(terminalsStore.get(TERMINAL_ID)?.activeBlock?.promptLine).toBe(5);
		});
		fire(`pty-parsed-${SESSION_ID}`, { type: "agent-block", action: "end", line: 9 });

		await waitFor(() => {
			const blocks = terminalsStore.get(TERMINAL_ID)?.commandBlocks ?? [];
			expect(blocks.length).toBe(1);
		});
		expect(terminalsStore.get(TERMINAL_ID)!.commandBlocks[0].exitCode).toBeNull();

		unmount();
	});
});
