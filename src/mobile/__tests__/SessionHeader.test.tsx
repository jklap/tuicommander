// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

const { rpc } = vi.hoisted(() => ({ rpc: vi.fn() }));
vi.mock("../../transport", () => ({ rpc }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div data-testid="terminal-output">Output</div> }));
vi.mock("../components/CommandInput", () => ({ CommandInput: () => <textarea aria-label="Command" /> }));
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../components/CommandWidget", () => ({
	CommandWidget: (props: { sessionId: string }) => <div role="dialog">Commands for {props.sessionId}</div>,
}));
vi.mock("../components/IdeasOverlay", () => ({ IdeasOverlay: () => <div role="dialog">Ideas</div> }));
vi.mock("../components/SuggestChips", () => ({ SuggestChips: () => <div /> }));

function session(id: string, agentType: string, name: string): SessionInfo {
	return {
		session_id: id,
		cwd: "/repo/src",
		worktree_path: "/repo",
		worktree_branch: "feature/links",
		display_name: name,
		state: {
			agent_type: agentType,
			awaiting_input: false,
			rate_limited: false,
			last_activity_ms: Date.now() - 60_000,
			shell_state: "busy",
			agent_intent: "Inspect the repository",
			current_task: "Running focused tests",
			active_sub_tasks: 3,
			progress: 42,
		},
	};
}

// useMobileVoice probes get_hands_free_status on mount. Answering it like the headless
// `tuic-remote` (a rejection) hides the control and leaves no dictation import pending.
function rpcReplies(value: unknown) {
	rpc.mockImplementation((method: string) =>
		method === "get_hands_free_status" ? Promise.reject(new Error("404")) : Promise.resolve(value),
	);
}

afterEach(() => {
	cleanup();
	rpc.mockReset();
	// useMobileVoice probes get_hands_free_status on mount; an undefined return breaks its promise chain.
	rpcReplies({});
});

describe("mobile session header", () => {
	it("shows the Codex brand and name in one compact header, with session-scoped Progress in overflow", async () => {
		rpcReplies({
			project: "/repo",
			entries: [{ id: 1, type: "done", text: "Tests pass", ptyId: "codex-1" }],
			nextCursor: null,
		});
		const view = render(() => (
			<SessionDetailScreen
				session={session("codex-1", "codex", "Merge risk")}
				sessionExists
				onBack={() => {}}
				onOpenFiles={() => {}}
			/>
		));
		expect(view.getByText("Merge risk")).toBeTruthy();
		expect(view.getByLabelText("Codex logo").querySelector("svg path")?.getAttribute("d")).toMatch(/^M22\.282/);
		expect(view.queryByText("Inspect the repository")).toBeNull();
		expect(view.queryByText("Running focused tests")).toBeNull();
		expect(view.queryByRole("button", { name: "Session progress" })).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: "More session actions" }));
		await fireEvent.click(view.getByRole("button", { name: /Progress/ }));
		await waitFor(() =>
			expect(rpc).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: expect.objectContaining({ ptyId: "codex-1" }),
			}),
		);
		await waitFor(() => expect(view.getByText("Tests pass")).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: "Close session details" }));
		expect(rpc).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "codex-1" });
	});

	it("shows the Claude brand and this session's task count and intent history", async () => {
		rpcReplies({
			project: "/repo",
			entries: [{ id: 2, type: "intent", text: "Earlier audit", ptyId: "claude-2" }],
			nextCursor: null,
		});
		const view = render(() => (
			<SessionDetailScreen
				session={session("claude-2", "claude", "Audit workspace")}
				sessionExists
				onBack={() => {}}
				onOpenFiles={() => {}}
			/>
		));
		expect(view.getByLabelText("Claude logo").querySelector("svg path")?.getAttribute("d")).toMatch(/^m4\.7144/);
		await fireEvent.click(view.getByRole("button", { name: "Audit workspace" }));
		expect(view.getByText("Inspect the repository")).toBeTruthy();
		await fireEvent.click(view.getByRole("button", { name: "Close session details" }));
		await fireEvent.click(view.getByRole("button", { name: "Session tasks, 3 active" }));
		await waitFor(() =>
			expect(rpc).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: expect.objectContaining({ ptyId: "claude-2" }),
			}),
		);
		expect(view.queryByText("Running focused tests")).toBeNull();
		await waitFor(() => expect(view.getByText("Earlier audit")).toBeTruthy());
	});

	it("keeps secondary actions and the command widget reachable from overflow", async () => {
		const onOpenFiles = vi.fn();
		const view = render(() => (
			<SessionDetailScreen
				session={session("codex-1", "codex", "Merge risk")}
				sessionExists
				onBack={() => {}}
				onOpenFiles={onOpenFiles}
			/>
		));
		await fireEvent.click(view.getByRole("button", { name: "More session actions" }));
		await fireEvent.click(view.getByRole("button", { name: "Commands" }));
		expect(view.getByRole("dialog").textContent).toContain("codex-1");
		await fireEvent.click(view.getByRole("button", { name: "More session actions" }));
		await fireEvent.click(view.getByRole("button", { name: "Files" }));
		expect(onOpenFiles).toHaveBeenCalledOnce();
	});

	it("explains when a session has no project instead of showing another session's journal", async () => {
		const detached = { ...session("codex-3", "codex", "Detached"), cwd: null, worktree_path: null };
		const view = render(() => (
			<SessionDetailScreen session={detached} sessionExists onBack={() => {}} onOpenFiles={() => {}} />
		));
		await fireEvent.click(view.getByRole("button", { name: "More session actions" }));
		await fireEvent.click(view.getByRole("button", { name: /Progress/ }));
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/no registered repository/i));
		expect(rpc).not.toHaveBeenCalledWith("progress_list", expect.anything());
	});
});
