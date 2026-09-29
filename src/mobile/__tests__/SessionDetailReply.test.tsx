import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

const { rpc } = vi.hoisted(() => ({
	rpc: vi.fn(async (_command: string, _args: Record<string, unknown>) => ({ submitted: true, acknowledged: true })),
}));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc,
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div /> }));
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));

afterEach(() => {
	cleanup();
	rpc.mockClear();
});

it("submits a root coordinator's answer atomically from the deep-linked session", async () => {
	const session: SessionInfo = {
		session_id: "coordinator-session",
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "codex",
			awaiting_input: true,
			rate_limited: false,
			last_activity_ms: 1,
		},
	};
	const { container } = render(() => (
		<SessionDetailScreen session={session} sessionExists={true} onBack={() => {}} onOpenFiles={() => {}} />
	));
	await fireEvent.input(container.querySelector("textarea")!, { target: { value: "Approve the change" } });
	expect(rpc).not.toHaveBeenCalled();
	await fireEvent.click(container.querySelector("button[type=button]")!);
	await waitFor(() =>
		expect(rpc).toHaveBeenCalledWith("submit_agent_reply", {
			sessionId: "coordinator-session",
			input: "Approve the change",
		}),
	);
});

it("opens a waiting Codex question through the session's PTY", async () => {
	const session: SessionInfo = {
		session_id: "codex-question",
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "codex",
			awaiting_input: true,
			question_confident: true,
			rate_limited: false,
			last_activity_ms: 1,
		},
	};
	const { getByRole } = render(() => (
		<SessionDetailScreen session={session} sessionExists={true} onBack={() => {}} onOpenFiles={() => {}} />
	));
	await fireEvent.click(getByRole("button", { name: "Open Codex question" }));
	await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", {
		sessionId: "codex-question",
		data: "\x1b[1;3A",
	}));
});

it("types into an opened Codex free-form question through the PTY", async () => {
	const session: SessionInfo = {
		session_id: "codex-free-form",
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "codex",
			awaiting_input: true,
			question_confident: true,
			rate_limited: false,
			last_activity_ms: 1,
		},
	};
	const { getByRole, container } = render(() => (
		<SessionDetailScreen session={session} sessionExists={true} onBack={() => {}} onOpenFiles={() => {}} />
	));
	await fireEvent.click(getByRole("button", { name: "Open Codex question" }));
	await fireEvent.input(container.querySelector("textarea")!, { target: { value: "Custom answer" } });
	await fireEvent.click(container.querySelector("button[type=button]")!);
	await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", {
		sessionId: "codex-free-form",
		data: "Custom answer",
	}));
	await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", {
		sessionId: "codex-free-form",
		data: "\r",
	}));
	expect(rpc.mock.calls.some(([command]) => command === "submit_agent_reply")).toBe(false);
});

it.each([
	["idle", "Concocting", true, false],
	["awaiting input", "Actioning", true, true],
	["ended", "Envisioning", false, false],
])("hides a bare spinner task while %s", (_state, task, sessionExists, awaitingInput) => {
	const session: SessionInfo = {
		session_id: "session",
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "claude",
			current_task: task,
			awaiting_input: awaitingInput,
			rate_limited: false,
			last_activity_ms: 1,
		},
	};
	const view = render(() => (
		<SessionDetailScreen session={session} sessionExists={sessionExists} onBack={() => {}} onOpenFiles={() => {}} />
	));
	expect(view.container.querySelector("header")).not.toBeNull();
	expect(view.queryByText(task)).toBeNull();
});

it.each(["Reading files", "Waiting for background terminal"])("shows a real task: %s", (task) => {
	const session: SessionInfo = {
		session_id: "session",
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "codex",
			current_task: task,
			awaiting_input: false,
			rate_limited: false,
			last_activity_ms: 1,
		},
	};
	const view = render(() => (
		<SessionDetailScreen session={session} sessionExists={true} onBack={() => {}} onOpenFiles={() => {}} />
	));
	expect(view.getByText(task)).not.toBeNull();
});
