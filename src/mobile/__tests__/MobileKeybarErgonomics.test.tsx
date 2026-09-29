// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import { SlashMenuOverlay } from "../components/SlashMenuOverlay";
import { TerminalKeybar } from "../components/TerminalKeybar";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

const { rpc } = vi.hoisted(() => ({ rpc: vi.fn(async () => undefined) }));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc,
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div>Terminal output</div> }));

afterEach(() => {
	cleanup();
	rpc.mockClear();
});

describe("mobile terminal controls", () => {
	it("shows Claude's actual choices without generic Yes or No buttons", () => {
		const session: SessionInfo = {
			session_id: "claude-question",
			cwd: "/repo",
			worktree_path: null,
			worktree_branch: null,
			state: {
				agent_type: "claude",
				awaiting_input: true,
				question_confident: true,
				rate_limited: false,
				last_activity_ms: 1,
				choice_prompt: {
					title: "Which color do you prefer?",
					selection_mode: "navigate-enter",
					options: [
						{ key: "1", label: "Red", highlighted: true, destructive: false },
						{ key: "2", label: "Green", highlighted: false, destructive: false },
					],
				},
			},
		};
		render(() => (
			<SessionDetailScreen session={session} sessionExists={true} onBack={() => {}} onOpenFiles={() => {}} />
		));
		expect(screen.getByRole("button", { name: /2\s*Green/ })).toBeTruthy();
		expect(screen.queryByRole("button", { name: "Yes" })).toBeNull();
		expect(screen.queryByRole("button", { name: "No" })).toBeNull();
	});

	it("keeps generic confirmations for an awaiting question without choices", () => {
		render(() => <TerminalKeybar sessionId="plain-question" agentType="claude" awaitingInput={true} questionConfident={true} />);
		expect(screen.getByRole("button", { name: "Yes" })).toBeTruthy();
		expect(screen.getByRole("button", { name: "No" })).toBeTruthy();
	});

	it("opens the keybar slash choices without typing into the agent until a choice is picked", async () => {
		let triggerSlash: (() => void) | undefined;
		render(() => (
			<>
				<TerminalKeybar sessionId="session-1" agentType="claude" onSlashRequest={() => triggerSlash?.()} />
				<CommandInput
					sessionId="session-1"
					agentType="claude"
					onRegisterTrigger={(fn) => {
						triggerSlash = fn;
					}}
				/>
			</>
		));
		fireEvent.click(screen.getByRole("button", { name: "/" }));
		expect(rpc).not.toHaveBeenCalledWith("write_pty", expect.anything());
		fireEvent.click(screen.getByRole("button", { name: "Send" }));
		await new Promise((resolve) => setTimeout(resolve, 250));
		expect(rpc).not.toHaveBeenCalledWith("write_pty", expect.anything());
		fireEvent.click(await screen.findByRole("button", { name: "/help" }));
		await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "session-1", data: "/help " }));
	});

	it("restores an unsent draft when the local slash menu is closed", () => {
		let triggerSlash: (() => void) | undefined;
		render(() => (
			<CommandInput
				sessionId="session-1"
				agentType="claude"
				onRegisterTrigger={(fn) => {
					triggerSlash = fn;
				}}
			/>
		));
		const input = screen.getByPlaceholderText("Type a command...") as HTMLTextAreaElement;
		fireEvent.input(input, { target: { value: "unsent draft" } });
		expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "session-1", data: "unsent draft" });
		rpc.mockClear();
		triggerSlash?.();
		expect(input.value).toBe("/");
		triggerSlash?.();
		fireEvent.click(screen.getByRole("button", { name: "Close slash menu" }));
		expect(input.value).toBe("unsent draft");
		expect(rpc).not.toHaveBeenCalledWith("write_pty", expect.anything());
	});

	it("keeps Tab and Escape local while the keybar slash menu is open", () => {
		let triggerSlash: (() => void) | undefined;
		render(() => (
			<CommandInput
				sessionId="session-1"
				agentType="claude"
				onRegisterTrigger={(fn) => {
					triggerSlash = fn;
				}}
			/>
		));
		const input = screen.getByPlaceholderText("Type a command...") as HTMLTextAreaElement;
		triggerSlash?.();
		fireEvent.keyDown(input, { key: "Tab" });
		expect(rpc).not.toHaveBeenCalledWith("write_pty", expect.anything());
		fireEvent.keyDown(input, { key: "Escape" });
		expect(input.value).toBe("");
		expect(rpc).not.toHaveBeenCalledWith("write_pty", expect.anything());
	});

	it("omits removed slash commands and closes the menu without sending a choice", () => {
		const onSelect = vi.fn();
		const onClose = vi.fn();
		render(() => (
			<SlashMenuOverlay
				items={[
					{ command: "/help", description: "Help", highlighted: false },
					{ command: "/agents", description: "Manage agents (removed)", highlighted: false },
				]}
				sessionId="session-1"
				onSelect={onSelect}
				onClose={onClose}
			/>
		));
		expect(screen.getByRole("button", { name: /\/help/ })).toBeTruthy();
		expect(screen.queryByRole("button", { name: /\/agents/ })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Close slash menu" }));
		expect(onClose).toHaveBeenCalledOnce();
		expect(onSelect).not.toHaveBeenCalled();
	});

	it("disables terminal controls and hides the activity badge after a session ends", () => {
		const session: SessionInfo = {
			session_id: "session-1",
			cwd: "/repo",
			worktree_path: null,
			worktree_branch: null,
			state: {
				agent_type: "claude",
				shell_state: "busy",
				awaiting_input: false,
				rate_limited: false,
				last_activity_ms: 1,
			},
		};
		render(() => (
			<SessionDetailScreen session={session} sessionExists={false} onBack={() => {}} onOpenFiles={() => {}} />
		));
		expect(screen.getByText("Session ended")).toBeTruthy();
		expect((screen.getByPlaceholderText("Type a command...") as HTMLTextAreaElement).disabled).toBe(true);
		expect((screen.getByRole("button", { name: "Ctrl+C" }) as HTMLButtonElement).disabled).toBe(true);
		expect((screen.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled).toBe(true);
		expect(screen.queryByText("Activity")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Ctrl+C" }));
		expect(rpc).not.toHaveBeenCalledWith("write_pty", expect.anything());
	});
});
