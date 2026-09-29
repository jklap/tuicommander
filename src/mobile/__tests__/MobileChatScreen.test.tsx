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

import { aiChatDraft } from "../../components/AIChatPanel/draft";
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
		ensureStarted: vi.fn(async () => {}),
		send: vi.fn(),
		cancel: vi.fn(),
		cancelQueued: vi.fn(),
		recover: vi.fn(),
	};
}

beforeEach(() => {
	aiChatDraft.reset();
	history.replaceState(null, "", "/mobile");
	invoke.mockReset();
	createAcpChat.mockReset().mockReturnValue(chat());
	answerPermission.mockClear();
	selectSession.mockClear();
});
afterEach(() => {
	cleanup();
	history.replaceState(null, "", "/mobile");
	vi.unstubAllGlobals();
});

describe("mobile ego chat", () => {
	it("passes a tapped transcript file link with the chat workspace to mobile navigation", async () => {
		const onOpenFile = vi.fn();
		createAcpChat.mockReturnValue({
			...chat(),
			entries: () => [{ id: "report", kind: "agent", text: "Read docs/guide.md" }],
		});
		render(() => <MobileChatScreen onOpenFile={onOpenFile} />);
		fireEvent.click(await screen.findByRole("link", { name: "docs/guide.md" }));
		expect(onOpenFile).toHaveBeenCalledWith("docs/guide.md", "/home/boss/Gits");
	});
	it("shows a message instead of guessing a path when the chat workspace is unavailable", async () => {
		const onOpenFile = vi.fn();
		createAcpChat.mockReturnValue({
			...chat(),
			root: () => null,
			entries: () => [{ id: "report", kind: "agent", text: "Read docs/guide.md" }],
		});
		render(() => <MobileChatScreen onOpenFile={onOpenFile} />);
		fireEvent.click(await screen.findByRole("link", { name: "docs/guide.md" }));
		expect(screen.getByRole("alert").textContent).toContain("workspace");
		expect(onOpenFile).not.toHaveBeenCalled();
	});

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

	it("offers one picker for photos and files without forcing camera capture", () => {
		const { container } = render(() => <MobileChatScreen />);
		const pickers = container.querySelectorAll('input[type="file"]');
		expect(pickers).toHaveLength(1);
		expect(pickers[0]).toBeInstanceOf(HTMLInputElement);
		expect(pickers[0].hasAttribute("capture")).toBe(false);
		expect(pickers[0].getAttribute("accept")).toContain("image/*");
	});

	it("keeps a shared document in the draft until Send", async () => {
		const current = chat();
		createAcpChat.mockReturnValue(current);
		vi.stubGlobal(
			"fetch",
			vi.fn(
				async () =>
					new Response(JSON.stringify({ path: "/repo/.tuic/attachments/1-report.pdf", size: 5 }), { status: 200 }),
			),
		);
		const { container } = render(() => <MobileChatScreen />);
		const picker = container.querySelector('input[type="file"]') as HTMLInputElement;
		fireEvent.change(picker, { target: { files: [new File(["report"], "report.pdf", { type: "application/pdf" })] } });
		await waitFor(() => expect(screen.getByText("report.pdf")).toBeTruthy());
		expect(current.send).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Send" }));
		expect(current.send).toHaveBeenCalledWith(
			"",
			[],
			[{ name: "report.pdf", path: "/repo/.tuic/attachments/1-report.pdf" }],
		);
	});

	it("recovers an Android share into the chat draft and removes its cached copy", async () => {
		history.replaceState(null, "", "/mobile?shared=shared-1");
		const remove = vi.fn(async () => true);
		vi.stubGlobal("caches", {
			open: async () => ({
				match: async () =>
					new Response("report", { headers: { "x-file-name": "report.pdf", "content-type": "application/pdf" } }),
				delete: remove,
			}),
		});
		vi.stubGlobal(
			"fetch",
			vi.fn(
				async () =>
					new Response(JSON.stringify({ path: "/repo/.tuic/attachments/2-report.pdf", size: 6 }), { status: 200 }),
			),
		);
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(screen.getByText("report.pdf")).toBeTruthy());
		expect(remove).toHaveBeenCalledWith("/_shared/shared-1");
		expect(location.search).toBe("");
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

	it("parks and restores a phone draft using the composer control", async () => {
		const { container } = render(() => <MobileChatScreen />);
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		fireEvent.input(textarea, { target: { value: "Phone draft" } });
		fireEvent.click(screen.getByRole("button", { name: "Park draft" }));
		expect(textarea.value).toBe("");
		expect(screen.getByText("Parked draft")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Restore parked draft" }));
		expect(textarea.value).toBe("Phone draft");
	});

	it("parks an uploaded document and restores it as an unsent attachment", async () => {
		const current = chat();
		createAcpChat.mockReturnValue(current);
		vi.stubGlobal(
			"fetch",
			vi.fn(
				async () =>
					new Response(JSON.stringify({ path: "/repo/.tuic/attachments/1-report.pdf", size: 6 }), { status: 200 }),
			),
		);
		const { container } = render(() => <MobileChatScreen />);
		const picker = container.querySelector('input[type="file"]') as HTMLInputElement;
		fireEvent.change(picker, { target: { files: [new File(["report"], "report.pdf", { type: "application/pdf" })] } });
		await waitFor(() => expect(screen.getByText("report.pdf")).toBeTruthy());
		fireEvent.click(screen.getByRole("button", { name: "Park draft" }));
		expect(screen.queryByText("report.pdf")).toBeNull();
		expect(current.send).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Restore parked draft" }));
		expect(screen.getByText("report.pdf")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Send" }));
		expect(current.send).toHaveBeenCalledWith(
			"",
			[],
			[{ name: "report.pdf", path: "/repo/.tuic/attachments/1-report.pdf" }],
		);
	});
});
