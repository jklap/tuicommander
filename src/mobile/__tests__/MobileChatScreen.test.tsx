import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, hydrate, createAcpChat, answerPermission, selectSession } = vi.hoisted(() => ({
	invoke: vi.fn(),
	hydrate: vi.fn(async () => {}),
	createAcpChat: vi.fn(),
	answerPermission: vi.fn(async () => {}),
	selectSession: vi.fn(async () => {}),
}));
vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../stores/settings", () => ({ settingsStore: { hydrate } }));
vi.mock("../../components/AIChatPanel/useAcpChat", () => ({ createAcpChat }));

import { MobileChatScreen } from "../screens/MobileChatScreen";

function chat() {
	return {
		phase: () => "live",
		root: () => "/home/boss/Gits",
		connectionId: () => "connection-1",
		sessionId: () => "current",
		entries: () => [
			{ id: "u", kind: "user", text: "Check the build" },
			{ id: "a", kind: "agent", text: "The build passed." },
			{ id: "t", kind: "tool", call: { toolCallId: "tool-1", title: "Run tests", status: "completed" } },
		],
		busy: () => false,
		queuedPrompts: () => [] as { turnId: string; summary: string }[],
		held: () => false,
		gap: () => null,
		error: () => null,
		isStreaming: () => true,
		interactions: () => [
			{
				kind: "permission",
				requestId: "permission-1",
				sessionId: "current",
				request: {
					toolCall: { title: "Write report" },
					options: [{ optionId: "allow", name: "Allow once", kind: "allow_once" }],
				},
			},
		],
		sessions: () => [
			{ sessionId: "current", cwd: "/repo", title: "Current work" },
			{ sessionId: "previous", cwd: "/repo", title: "Earlier review" },
		],
		capabilities: () => ({ list: true, load: true }),
		configOptions: () => [],
		answerPermission,
		selectSession,
		answerElicitation: vi.fn(),
		cancelPermission: vi.fn(),
		startSession: vi.fn(),
		send: vi.fn(),
		cancel: vi.fn(),
		cancelQueued: vi.fn(),
		recover: vi.fn(),
	};
}

beforeEach(() => {
	history.replaceState(null, "", "/mobile");
	invoke.mockReset();
	createAcpChat.mockReset().mockReturnValue(chat());
	answerPermission.mockClear();
	selectSession.mockClear();
});
afterEach(() => {
	cleanup();
	history.replaceState(null, "", "/mobile");
});

describe("mobile ego chat", () => {
	it("opens global chat without a repository selection or repository fetch", async () => {
		render(() => <MobileChatScreen />);
		expect(screen.queryByRole("combobox", { name: "Repository" })).toBeNull();
		expect(screen.getByRole("combobox", { name: "Conversation" })).toBeTruthy();
		expect(invoke).not.toHaveBeenCalledWith("load_repositories");
		expect(createAcpChat.mock.calls[0][0]()).toBeNull();
	});

	it("opens a linked conversation without a repository in the URL", async () => {
		history.replaceState(null, "", "/mobile?session=previous");
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
	});

	it("opens a push-linked conversation with its repository as a context hint", async () => {
		history.replaceState(null, "", "/mobile?repo=%2Frepo&session=previous");
		render(() => <MobileChatScreen />);
		expect(screen.queryByRole("combobox", { name: "Repository" })).toBeNull();
		expect(createAcpChat.mock.calls[0][0]()).toBe("/repo");
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
	});

	// The chat is global and nothing starts ego before the first message, so a
	// link must not wait for a live connection or a listed session.
	it("selects a linked conversation before ego is running, and lets the first message start it", async () => {
		history.replaceState(null, "", "/mobile?repo=%2Frepo&session=previous");
		createAcpChat.mockReturnValue({
			...chat(),
			phase: () => "ready",
			connectionId: () => null,
			sessionId: () => null,
			sessions: () => [],
		});
		const { container } = render(() => <MobileChatScreen />);
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
		expect(container.querySelector("textarea")).toBeTruthy();
	});

	it("opens a push-linked conversation without loading a local repository list", async () => {
		history.replaceState(null, "", "/mobile?repo=%2Funknown&session=previous");
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
		expect(invoke).not.toHaveBeenCalledWith("load_repositories");
	});

	it("shows the conversation, card and collapsed activity on direct open", async () => {
		const { container } = render(() => <MobileChatScreen />);
		await waitFor(() => expect(screen.getByText("The build passed.")).toBeTruthy());
		expect(screen.getByText("Check the build")).toBeTruthy();
		const activity = screen.getByText(/1 tool call/).closest("details");
		expect(activity?.open).toBe(false);
		expect(activity?.textContent).toContain("Run tests");
		expect(container.querySelector("textarea")).toBeTruthy();
	});

	it("sends one answer when the permission button is tapped twice", async () => {
		render(() => <MobileChatScreen />);
		const button = await screen.findByRole("button", { name: "Allow once" });
		fireEvent.click(button);
		fireEvent.click(button);
		expect(answerPermission).toHaveBeenCalledTimes(1);
		expect(answerPermission).toHaveBeenCalledWith("permission-1", "allow");
	});

	it("lists titled conversations and resumes the selected one", async () => {
		render(() => <MobileChatScreen />);
		const picker = await screen.findByRole("combobox", { name: "Conversation" });
		expect(screen.getByRole("option", { name: "Earlier review" })).toBeTruthy();
		fireEvent.change(picker, { target: { value: "previous" } });
		expect(selectSession).toHaveBeenCalledWith("previous");
	});

	it("shows the shared queue and cancels a selected prompt from phone", async () => {
		const shared = chat();
		shared.busy = () => true;
		shared.queuedPrompts = () => [{ turnId: "phone-turn", summary: "Follow up from desktop" }];
		createAcpChat.mockReturnValue(shared);
		render(() => <MobileChatScreen />);

		expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();
		expect(screen.getByRole("button", { name: "Queue" })).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Cancel queued prompt Follow up from desktop" }));
		expect(shared.cancelQueued).toHaveBeenCalledWith("phone-turn");
	});
});
