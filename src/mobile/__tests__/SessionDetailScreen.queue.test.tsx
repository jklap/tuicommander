import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { QueuedCommand } from "../../hooks/usePty";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc: vi.fn(),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div>Terminal output</div> }));
vi.mock("../../components/Terminal/TerminalChatTranscript", () => ({
	TerminalChatTranscript: () => <div>Conversation</div>,
}));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({ phase: () => null, available: () => false, armed: () => false, recording: () => false }),
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));

import { rpc } from "../../transport";

let queue: QueuedCommand[];
let nextId = 1;
let warnings: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
	queue = [];
	warnings = vi.spyOn(console, "warn").mockImplementation(() => {});
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command === "list_queued_agent_commands") return [...queue];
		if (command === "enqueue_agent_command") {
			queue.push({ id: nextId++, text: args!.text as string, kind: "user_command" });
			return { accepted: true, typed: false };
		}
		if (command === "remove_queued_agent_command") {
			queue = queue.filter((item) => item.id !== args!.commandId);
			return true;
		}
		if (command === "clear_queued_agent_commands") {
			queue = [];
			return 1;
		}
		return undefined;
	});
});
afterEach(() => {
	cleanup();
	for (const args of warnings.mock.calls)
		expect(args).toEqual(["computations created outside a `createRoot` or `render` will never be disposed"]);
	warnings.mockRestore();
	vi.clearAllMocks();
});
function mount() {
	const [session, setSession] = createSignal<SessionInfo>({
		session_id: `queue-${nextId++}`,
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "claude",
			shell_state: "busy",
			awaiting_input: false,
			rate_limited: false,
			last_activity_ms: 0,
		},
	});
	const view = render(() => (
		<SessionDetailScreen
			session={session()}
			sessionExists
			subscribeChatWake={() => () => {}}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	return {
		...view,
		session,
		setSession,
		updateCount: (count: number) => setSession((s) => ({ ...s, state: { ...s.state!, queued_commands: count } })),
	};
}
async function enqueue(view: ReturnType<typeof mount>, text: string) {
	fireEvent.input(view.getByRole("textbox"), { target: { value: text } });
	fireEvent.click(view.getByRole("button", { name: "More composer actions" }));
	fireEvent.click(view.getByRole("menuitem", { name: "Queue" }));
	await waitFor(() => expect((view.getByRole("textbox") as HTMLTextAreaElement).value).toBe(""));
}

// Catches: successful queue clears the input but leaves Chat with only a transient toast.
it("shows queued text immediately before the lifecycle count arrives, then clears on delivery", async () => {
	const view = mount();
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	await enqueue(view, "Review the migration");
	const region = await view.findByRole("region", { name: "Queued prompts" });
	expect(within(region).getByRole("status").textContent).toBe("Queued · 1");
	expect(within(region).getByText("Review the migration")).toBeTruthy();
	expect(vi.mocked(rpc).mock.calls.some(([cmd, args]) => cmd === "write_pty" && args?.data === "\r")).toBe(false);
	view.updateCount(1);
	queue = [];
	view.updateCount(0);
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
});

// Catches: Chat invents its own empty queue and loses entries added in CLI or desktop.
it("keeps queued CLI text on switching to Chat and adopts desktop queue count changes", async () => {
	const view = mount();
	await enqueue(view, "Queued in CLI");
	await view.findByText("Queued in CLI");
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	expect(view.getByText("Queued in CLI")).toBeTruthy();
	queue.push({ id: nextId++, kind: "user_command", text: "Queued on desktop" });
	view.updateCount(2);
	await view.findByText("Queued on desktop");
	expect(view.getByRole("status").textContent).toBe("Queued · 2");
});

// Catches: cancellation removes by display position, or a successful clear leaves stale text.
it("cancels stable queue ids and clears remaining entries from the server snapshot", async () => {
	const view = mount();
	queue = [
		{ id: 41, text: "First task", kind: "user_command" },
		{ id: 85, text: "Second task", kind: "user_command" },
	];
	view.updateCount(2);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	fireEvent.click(await view.findByRole("button", { name: "Cancel queued prompt First task" }));
	await waitFor(() => expect(view.queryByText("First task")).toBeNull());
	expect(rpc).toHaveBeenCalledWith("remove_queued_agent_command", {
		sessionId: view.session().session_id,
		commandId: 41,
	});
	expect(view.getByText("Second task")).toBeTruthy();
	fireEvent.click(view.getByRole("button", { name: "Clear all" }));
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
});

// Catches: a slow queue read resurrects prompts from a previously selected terminal.
it("does not render a departed terminal's late queue snapshot", async () => {
	const view = mount();
	let resolveOld!: (items: QueuedCommand[]) => void;
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command !== "list_queued_agent_commands") return undefined;
		if (args?.sessionId === "next-queue") return [];
		return new Promise<QueuedCommand[]>((resolve) => {
			resolveOld = resolve;
		});
	});
	view.updateCount(1);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	await waitFor(() => expect(resolveOld).toBeTypeOf("function"));
	view.setSession((s) => ({ ...s, session_id: "next-queue" }));
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	resolveOld([{ id: 23, text: "Wrong terminal task", kind: "user_command" }]);
	await Promise.resolve();
	expect(view.queryByText("Wrong terminal task")).toBeNull();
});

// Catches: a failed cancel hides an undelivered prompt or leaks a rejected promise.
it("keeps queued text when cancellation fails and explains the failure", async () => {
	const view = mount();
	queue = [{ id: 91, text: "Keep this task", kind: "user_command" }];
	view.updateCount(1);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	const cancel = await view.findByRole("button", { name: "Cancel queued prompt Keep this task" });
	vi.mocked(rpc).mockImplementation(async (command) => {
		if (command === "remove_queued_agent_command") throw new Error("offline");
		return [...queue];
	});
	fireEvent.click(cancel);
	await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Could not remove queued prompts"));
	expect(view.getByText("Keep this task")).toBeTruthy();
});
