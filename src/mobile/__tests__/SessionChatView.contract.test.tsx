import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, listen, warnings } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), warnings: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: warnings, error: vi.fn(), info: vi.fn() } }));

import { TerminalChatView, ViewModeToggle } from "../../components/Terminal/TerminalChatView";
import { chatViewStore } from "../../stores/chatView";
import { terminalsStore } from "../../stores/terminals";
import { HttpRpcError } from "../../transport/http";
import { useSessionChatView } from "../components/useSessionChatView";

let wake: () => void;
let release: ReturnType<typeof vi.fn<() => void>>;
const snapshot = (text: string, epoch = 1) => ({
	epoch,
	nextSeq: 1,
	reset: true,
	unknownRows: 0,
	malformedRows: 0,
	updates: [{ sessionUpdate: "agent_message_chunk", messageId: `answer-${epoch}`, content: { type: "text", text } }],
});
beforeEach(() => {
	vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
	chatViewStore.reset();
	invoke.mockReset();
	warnings.mockClear();
	release = vi.fn();
	listen.mockImplementation(async (_event, fn) => {
		wake = () => fn({ payload: { session_id: "contract" } });
		return release;
	});
	invoke.mockResolvedValue(snapshot("First answer"));
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.unstubAllGlobals();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
});

function mount(host: "desktop" | "mobile") {
	const [agent, setAgent] = createSignal<string | undefined>("claude");
	if (host === "desktop") {
		const id = terminalsStore.add({
			sessionId: "contract",
			cwd: "/repo",
			name: "Claude",
			fontSize: 14,
			awaitingInput: null,
			agentType: "claude",
			agentSessionId: null,
		});
		const view = render(() => (
			<>
				<ViewModeToggle terminalId={id} />
				<TerminalChatView terminalId={id} sessionId="contract" />
			</>
		));
		return {
			...view,
			exit: () => terminalsStore.update(id, { agentType: null }),
			mode: () => terminalsStore.get(id)?.viewMode,
		};
	}
	const view = render(() => {
		const chat = useSessionChatView({
			sessionId: () => "contract",
			agentType: agent,
			sessionExists: () => true,
			subscribeWake: (_id, fn) => {
				wake = fn;
				return release;
			},
		});
		return (
			<>
				<chat.Toggle />
				<chat.View />
			</>
		);
	});
	return {
		...view,
		exit: () => setAgent(undefined),
		mode: () => (view.queryByText("CLI")?.getAttribute("aria-pressed") === "false" ? "chat" : "cli"),
	};
}

// Desktop-only file navigation/search are covered by TerminalChatView.test and chatViewSearch.test.
describe.each(["desktop", "mobile"] as const)("%s terminal chat contract", (host) => {
	// Catches: a shared transcript hint prop/default reintroduces invented guidance.
	it("renders Chat without invented guidance", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		expect(view.container.textContent).not.toMatch(/Reply below|Switch to CLI|Read file edits in Chat/);
	});
	// Catches: frontend missing-id gate prevents backend-owned binding; epoch reset leaves old prose visible.
	it("opens Claude without a discovered id and replaces the old epoch on a wake", async () => {
		const view = mount(host);
		const chat = view.getByText("Chat") as HTMLButtonElement;
		expect(chat.disabled).toBe(false);
		chat.click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		invoke.mockResolvedValue(snapshot("New conversation", 2));
		wake();
		await waitFor(() => expect(view.container.textContent).toContain("New conversation"));
		expect(view.container.textContent).not.toContain("First answer");
		view.unmount();
		expect(release).toHaveBeenCalled();
		const reads = invoke.mock.calls.length;
		vi.advanceTimersByTime(10_000);
		wake();
		expect(invoke.mock.calls.length).toBe(reads);
	});
	// Catches: HTTP not_bound is treated as an ordinary error and leaves a blank Chat panel.
	it("returns to CLI with the backend reason from an HTTP not_bound response", async () => {
		invoke.mockRejectedValue(
			new HttpRpcError("chat_view_snapshot", 500, '{"error":"not_bound: transcript unavailable"}'),
		);
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.mode()).toBe("cli"));
		expect(chatViewStore.unavailableReason("contract")).toBe("transcript unavailable");
		expect(warnings).toHaveBeenCalledWith(
			"terminal",
			"Chat view refresh failed",
			expect.objectContaining({ sessionId: "contract" }),
		);
	});
	// Catches: terminal death leaves the host stuck in Chat.
	it("returns to CLI when the agent exits", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		view.exit();
		await waitFor(() => expect(view.mode()).toBe("cli"));
	});
	// Catches: hidden page keeps polling or misses the full reset on resume.
	it("pauses hidden reads and catches up immediately on return", async () => {
		let hidden = false;
		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			get: () => (hidden ? "hidden" : "visible"),
		});
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		const reads = invoke.mock.calls.length;
		hidden = true;
		fireEvent(document, new Event("visibilitychange"));
		vi.advanceTimersByTime(15_000);
		wake();
		expect(invoke.mock.calls.length).toBe(reads);
		invoke.mockResolvedValue(snapshot("Resumed answer", 3));
		hidden = false;
		fireEvent(document, new Event("visibilitychange"));
		await waitFor(() => expect(view.container.textContent).toContain("Resumed answer"));
	});
});
