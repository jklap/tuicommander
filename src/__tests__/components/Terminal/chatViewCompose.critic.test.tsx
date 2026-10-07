import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
		resize: vi.fn(),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
	subscribePty: () => Promise.resolve(() => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
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

describe("Chat with an existing Compose editor", () => {
	// Catches: an editor opened in CLI remains interactive over the read-only Chat footer.
	it("hides_an_already_open_compose_editor_when_switching_to_chat", async () => {
		const id = terminalsStore.add({
			sessionId: "live-session",
			cwd: "/repo",
			repoPath: "/repo",
			name: "claude",
			fontSize: 13,
			awaitingInput: null,
		});
		terminalsStore.update(id, { agentType: "claude", agentSessionId: "uuid" });
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		fireEvent.click(view.getByText(/^Compose /));
		await view.findByLabelText("Close compose panel");
		terminalsStore.setViewMode(id, "chat");
		await Promise.resolve();
		expect(view.queryByLabelText("Close compose panel")).toBeNull();
		terminalsStore.setViewMode(id, "cli");
		await view.findByLabelText("Close compose panel");
	});
});
