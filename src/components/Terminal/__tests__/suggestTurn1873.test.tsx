import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => ({ onParsed: undefined as undefined | ((frame: Record<string, unknown>) => void) }));
vi.mock("../../../transport", () => ({
	isTauri: () => false,
	HttpRpcError: class extends Error {},
	rpc: vi.fn().mockResolvedValue(null),
	subscribePty: vi.fn(
		async (
			_sid: string,
			_data: unknown,
			_exit: unknown,
			options: { onParsed?: (frame: Record<string, unknown>) => void },
		) => {
			bridge.onParsed = options.onParsed;
			return () => {};
		},
	),
}));
vi.mock("../CanvasTerminal", () => ({ default: () => null }));

import { terminalsStore } from "../../../stores/terminals";
import { Terminal } from "../Terminal";
import capture from "./fixtures/codex-choice-events-1873.json";

let terminalId: string;
afterEach(() => {
	cleanup();
	if (terminalId) terminalsStore.remove(terminalId);
	vi.restoreAllMocks();
});
it("shows recorded next-turn choices when the submitted-input event follows busy", async () => {
	// Catches: a submitted prompt dismissing its own next-turn suggestions.
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
	const id = terminalsStore.add({
		sessionId: "recorded1873",
		name: "Codex",
		fontSize: 14,
		cwd: null,
		awaitingInput: null,
	});
	terminalId = id;
	render(() => <Terminal id={id} alwaysVisible />);
	await waitFor(() => expect(bridge.onParsed).toBeTypeOf("function"));
	for (const frame of capture.events) bridge.onParsed!(frame);
	expect(terminalsStore.get(id)?.suggestedActions).toEqual(["Inspect mapping", "Verify chips", "Finish capture"]);
});
