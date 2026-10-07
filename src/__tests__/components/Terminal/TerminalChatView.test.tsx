import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

import { TerminalChatView, ViewModeToggle } from "../../../components/Terminal/TerminalChatView";
import { chatViewStore } from "../../../stores/chatView";
import { terminalsStore } from "../../../stores/terminals";

function addTerminal(agentType: "claude" | null, agentSessionId: string | null) {
	return terminalsStore.add({
		sessionId: "sess-1",
		fontSize: 14,
		name: "t",
		cwd: "/repo",
		awaitingInput: null,
		agentType,
		agentSessionId,
	});
}

beforeEach(() => {
	chatViewStore.reset();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
});

describe("TerminalChatView", () => {
	// Catches: a blank panel after the agent dies (grid hidden, nothing in its place).
	it("view_falls_back_to_cli_when_agent_exits", async () => {
		const id = addTerminal("claude", "uuid");
		terminalsStore.setViewMode(id, "chat");
		render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
		expect(terminalsStore.get(id)?.viewMode).toBe("chat");

		terminalsStore.update(id, { agentType: null });
		await Promise.resolve();

		expect(terminalsStore.get(id)?.viewMode).toBe("cli");
		expect(chatViewStore.unavailableReason("sess-1")).toBeTruthy();
	});
});

describe("ViewModeToggle", () => {
	it("is absent for a plain shell tab", () => {
		const id = addTerminal(null, null);
		const { queryByRole } = render(() => <ViewModeToggle terminalId={id} />);
		expect(queryByRole("group", { name: "Terminal view" })).toBeNull();
	});

	it("disables Chat with the reason when the agent is not bound", () => {
		const id = addTerminal("claude", null);
		const { getByText } = render(() => <ViewModeToggle terminalId={id} />);
		const chat = getByText("Chat") as HTMLButtonElement;
		expect(chat.disabled).toBe(true);
		expect(chat.title).toContain("not bound");
	});

	it("switches the terminal to chat when bound", () => {
		const id = addTerminal("claude", "uuid");
		const { getByText } = render(() => <ViewModeToggle terminalId={id} />);
		(getByText("Chat") as HTMLButtonElement).click();
		expect(terminalsStore.get(id)?.viewMode).toBe("chat");
		(getByText("CLI") as HTMLButtonElement).click();
		expect(terminalsStore.get(id)?.viewMode).toBe("cli");
	});
});
