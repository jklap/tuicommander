import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

import { type SessionStateExplain, StateExplainModal } from "../../components/StateExplainModal/StateExplainModal";

const EXPLAIN: SessionStateExplain = {
	session_id: "abc12345-def6-7890",
	captured_at_ms: 1_700_000_000_000,
	agent: {
		agent_type: "claude",
		agent_seen_running: true,
		hook_instrumented: true,
		hook_state_seen: true,
		has_ready_screen_adapter: true,
	},
	visible: {
		shell_state: "busy",
		agent_state: "working",
		agent_state_rung: "shell_busy",
		awaiting_input: false,
		question_confident: false,
		choice_prompt_present: false,
		background_work: false,
		declared_background_work: false,
		rate_limited: false,
		queued_commands: 0,
		turn_epoch: 3,
		active_sub_tasks: 0,
	},
	evidence: {
		busy: { rank: "protocol", source: "hook-busy", age_ms: 1200 },
		idle: null,
		awaiting: null,
		activity_seen: true,
		idle_confirmed: false,
		shell_is_busy: true,
		decide_now: null,
	},
	screen: {
		cached_activity: "working",
		screen_ready_pending_since_ms: null,
		skipped_by_protocol_authority: true,
		no_adapter_for_agent: false,
	},
	silence: {
		last_output_ms_ago: 500,
		last_chunk_ms_ago: 500,
		threshold_ms: 2500,
		threshold_reason: "agent_type_present",
		remaining_before_fire_ms: 2000,
		startup_settled: true,
		last_status_line_ms_ago: 500,
	},
	notification: null,
	trail: [
		{
			seq: 0,
			age_ms: 4000,
			kind: "idle",
			rank: "protocol",
			source: "hook-idle",
			accepted: true,
			forced: false,
			outranked_by: null,
		},
		{
			seq: 1,
			age_ms: 1200,
			kind: "busy",
			rank: "screen",
			source: "working-screen",
			accepted: false,
			forced: false,
			outranked_by: { rank: "protocol", source: "hook-idle" },
		},
	],
};

describe("StateExplainModal", () => {
	const baseProps = () => ({
		sessionId: "abc12345-def6-7890",
		shellState: "busy" as string | null,
		awaitingInput: null as string | null,
		isRateLimited: false,
		agentState: "working" as string | null,
		backgroundWork: false,
		declaredBackgroundWork: false,
		onClose: vi.fn(),
	});

	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue(EXPLAIN);
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
	});

	it("fetches explain_session_state and renders the evidence + trail", async () => {
		const { container, getByText } = render(() => <StateExplainModal {...baseProps()} />);

		expect(container.querySelector(`.${"spinner"}`)).not.toBeNull();

		await waitFor(() => {
			expect(getByText("protocol · hook-busy (1.2s ago)")).not.toBeNull();
		});

		expect(mockInvoke).toHaveBeenCalledWith("explain_session_state", {
			sessionId: "abc12345-def6-7890",
		});
		// The rejected trail entry must name what outranked it.
		expect(getByText(/rejected, outranked by protocol hook-idle/)).not.toBeNull();
		// No banner: frontend/backend agree (both "working").
		expect(container.textContent).not.toContain("disagrees with backend");
	});

	it("shows the disagreement banner when the frontend badge and backend agent_state diverge", async () => {
		// shellState/agentState both "idle" (no backgroundWork) → frontend badge
		// is "idle", but the fixed EXPLAIN fixture's backend agent_state is
		// "working" — a real, non-carve-out mismatch.
		const props = {
			...baseProps(),
			shellState: "idle" as string | null,
			agentState: "idle" as string | null,
		};
		const { container } = render(() => <StateExplainModal {...props} />);

		await waitFor(() => {
			expect(container.textContent).toContain("disagrees with backend");
		});
	});

	it("does NOT show the disagreement banner for a rate-limited session, even though the frontend badge always reads rate_limited", async () => {
		// Code-review regression: rate_limited is orthogonal to agent_state —
		// every rate-limited session used to show a false-positive mismatch.
		const props = { ...baseProps(), isRateLimited: true };
		const { container, getByText } = render(() => <StateExplainModal {...props} />);

		await waitFor(() => {
			expect(getByText("protocol · hook-busy (1.2s ago)")).not.toBeNull();
		});
		expect(container.textContent).not.toContain("disagrees with backend");
		expect(container.textContent).toContain("rate_limited");
	});

	it("shows an error when the session is not found", async () => {
		mockInvoke.mockResolvedValueOnce(null);
		const { container } = render(() => <StateExplainModal {...baseProps()} />);

		await waitFor(() => {
			expect(container.textContent).toContain("Session not found");
		});
	});

	it("copies the payload as JSON via writeClipboard, not navigator.clipboard directly", async () => {
		const props = baseProps();
		const { getByText } = render(() => <StateExplainModal {...props} />);

		await waitFor(() => {
			expect(getByText("Copy as JSON")).not.toBeNull();
		});
		fireEvent.click(getByText("Copy as JSON"));

		// writeClipboard routes through Tauri's invoke in the mocked Tauri
		// environment — this is the exact call the WKWebView clipboard bug
		// (issue #101, HelpPanel.tsx) requires, in place of a direct
		// navigator.clipboard.writeText call inside a modal.
		await waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith(
				"plugin:clipboard-manager|write_text",
				expect.objectContaining({ text: expect.stringContaining('"session_id"') }),
			);
		});
	});

	it("calls onClose when the close button is clicked", async () => {
		const props = baseProps();
		const { getByTitle } = render(() => <StateExplainModal {...props} />);
		fireEvent.click(getByTitle("Close"));
		expect(props.onClose).toHaveBeenCalledTimes(1);
	});
});
