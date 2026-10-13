import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockOpenFile } = vi.hoisted(() => ({ mockOpenFile: vi.fn() }));

vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (command: string, args?: { cwd?: string; candidate?: string }) => {
		if (command === "resolve_terminal_path") {
			return args?.candidate === "reports/x.md"
				? { absolute_path: `${args.cwd}/reports/x.md`, is_directory: false }
				: null;
		}
		return { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 };
	}),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../../utils/filePreview", () => ({ openTerminalFilePath: mockOpenFile }));

import { TerminalChatView, ViewModeToggle } from "../../../components/Terminal/TerminalChatView";
import { acpTranscript } from "../../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../../stores/chatView";
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

describe("TerminalChatView file links", () => {
	const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
	async function renderAnswer(text: string) {
		const id = addTerminal("claude", "uuid");
		terminalsStore.setViewMode(id, "chat");
		const view = render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
		await settle();
		acpTranscript.restore(chatViewKey("sess-1"), [{ id: "a1", kind: "agent", text }]);
		await settle();
		return view;
	}

	// Catches: Chat view Transcript wired without onOpenFile, and a relative path not resolved against the terminal cwd.
	it("opens the absolute path of a backticked relative path on click", async () => {
		const { container } = await renderAnswer("Report in `reports/x.md` now.");
		const link = container.querySelector<HTMLAnchorElement>("a[data-tuic-href]");
		expect(link?.dataset.tuicHref).toBe("reports/x.md");
		link?.click();
		await settle();
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/reports/x.md");
	});

	// Catches: a click on a missing file opening something or throwing.
	it("opens nothing when the path does not exist", async () => {
		mockOpenFile.mockClear();
		const { container } = await renderAnswer("Gone: `reports/missing.md`.");
		container.querySelector<HTMLAnchorElement>("a[data-tuic-href]")?.click();
		await settle();
		expect(mockOpenFile).not.toHaveBeenCalled();
	});

	// Catches: a nonexistent target remaining a clickable dead link after resolution fails.
	it("removes the dead link when a clicked file no longer exists", async () => {
		const { container } = await renderAnswer("Gone: `reports/missing.md`.");
		const link = container.querySelector<HTMLAnchorElement>("a[data-tuic-href]");
		expect(link).not.toBeNull();
		link?.click();
		await settle();
		expect(container.textContent).toContain("reports/missing.md");
		expect(container.querySelector("a[data-tuic-href='reports/missing.md']")).toBeNull();
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
