import { render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
		resize: vi.fn(),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-testid="canvas" />,
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async () => ({
		epoch: 0,
		nextSeq: 0,
		reset: true,
		updates: [],
		unknownRows: 0,
		malformedRows: 0,
	})),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { terminalsStore } from "../../../stores/terminals";

/**
 * Chat mode must HIDE the grid and never unmount it: `CanvasTerminal` under a
 * disposed `<Show keyed>` with a frame event already queued froze the whole UI
 * (see the comment above that `<Show>` in Terminal.tsx).
 */
describe("chat view toggle", () => {
	// Catches: the Show-keyed disposal freeze / lost scroll when switching to chat.
	it("toggle_hides_canvas_without_unmounting_it", async () => {
		const id = terminalsStore.add({
			sessionId: null,
			cwd: "/tmp/repo",
			repoPath: null,
			name: "claude",
			fontSize: 13,
			awaitingInput: null,
			agentType: "claude",
			agentSessionId: "uuid",
		});
		const { findByTestId } = render(() => <Terminal id={id} cwd="/tmp/repo" alwaysVisible />);
		const canvas = await findByTestId("canvas");
		const hiddenAncestor = () => canvas.closest('[class*="contentHidden"]');
		expect(hiddenAncestor()).toBeNull();

		terminalsStore.setViewMode(id, "chat");
		await Promise.resolve();

		expect(canvas.isConnected).toBe(true);
		expect(hiddenAncestor()).not.toBeNull();
	});
});
