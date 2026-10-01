import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

import { type SessionStateExplain, StateExplainModal } from "../../components/StateExplainModal/StateExplainModal";

const EXPLAIN: SessionStateExplain = {
	session_id: "abc12345-def6-7890",
	captured_at_ms: 1_700_000_000_000,
	agent: {
		agent_type: "claude",
		agent_type_from_run_config: true,
		agent_foreground_observed: true,
		spawn_root_role: "shell",
		foreground_input_blocked: false,
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

	it("agrees with a backend 'working' (background_work rung) when the frontend has declaredBackgroundWork, even with an idle shell", async () => {
		mockInvoke.mockResolvedValue({
			...EXPLAIN,
			visible: {
				...EXPLAIN.visible,
				shell_state: "idle",
				agent_state: "working",
				agent_state_rung: "background_work",
				declared_background_work: true,
			},
		});
		const props = {
			...baseProps(),
			shellState: "idle" as string | null,
			agentState: "idle" as string | null,
			declaredBackgroundWork: true,
		};
		const { container, getByText } = render(() => <StateExplainModal {...props} />);

		await waitFor(() => {
			expect(getByText("protocol · hook-busy (1.2s ago)")).not.toBeNull();
		});
		expect(container.textContent).not.toContain("disagrees with backend");
	});

	it("shows the disagreement banner when the backend says working via declared_background_work but the frontend never got declaredBackgroundWork", async () => {
		// The idle-with-background-work carve-out is keyed on the OS-heuristic
		// `background_work` only; declared work is deliberately NOT part of it.
		mockInvoke.mockResolvedValue({
			...EXPLAIN,
			visible: {
				...EXPLAIN.visible,
				shell_state: "idle",
				agent_state: "working",
				agent_state_rung: "background_work",
				background_work: false,
				declared_background_work: true,
			},
		});
		const props = {
			...baseProps(),
			shellState: "idle" as string | null,
			agentState: "idle" as string | null,
			declaredBackgroundWork: false,
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

	describe("declared background work and swarm sections", () => {
		const DECLARED = {
			declared: true,
			declared_turn_epoch: 3,
			applies_now: true,
			age_ms: 125_000,
			breakdown_source: "summary" as const,
			non_teammate_running: 0,
			teammate_running: 2,
			teammates_busy: false,
			unlinked_teammates: 1,
			consistent_with_visible: true,
			counts_now: true,
		};
		const withDeclared = (over: Partial<typeof DECLARED> = {}, swarm: SessionStateExplain["swarm"] = null) => ({
			...EXPLAIN,
			epoch_flags: { declared_background_work: { ...DECLARED, ...over } },
			swarm,
		});

		it("renders the breakdown, age and the unlinked-teammate warning", async () => {
			mockInvoke.mockResolvedValue(withDeclared());
			const { container, getByText } = render(() => <StateExplainModal {...baseProps()} />);
			await waitFor(() => expect(getByText("Declared background work")).not.toBeNull());
			const text = container.textContent ?? "";
			expect(text).toContain("holds the session working");
			expect(text).toContain("125.0s ago");
			expect(text).toContain("summary");
			expect(text).toContain("no linked pane accounts for them");
			expect(text).not.toContain("Captured mid-change");
		});

		it("says so when the capture was torn (counts_now disagrees with the visible value)", async () => {
			mockInvoke.mockResolvedValue(withDeclared({ consistent_with_visible: false }));
			const { container } = render(() => <StateExplainModal {...baseProps()} />);
			await waitFor(() => expect(container.textContent).toContain("Captured mid-change"));
		});

		it("shows an idle-teammate declaration as not holding the session working", async () => {
			mockInvoke.mockResolvedValue(
				withDeclared({ counts_now: false, unlinked_teammates: 0, teammates_busy: false, teammate_running: 1 }),
			);
			const { container } = render(() => <StateExplainModal {...baseProps()} />);
			await waitFor(() => expect(container.textContent).toContain("does not hold it working"));
			expect(container.textContent).not.toContain("no linked pane accounts for them");
		});

		it("omits the section when nothing is declared, and tolerates a backend that predates the fields", async () => {
			mockInvoke.mockResolvedValue(withDeclared({ declared: false, counts_now: false }));
			const first = render(() => <StateExplainModal {...baseProps()} />);
			await waitFor(() => expect(first.container.textContent).toContain("Evidence"));
			expect(first.container.textContent).not.toContain("Declared background work");
			cleanup();

			mockInvoke.mockResolvedValue(EXPLAIN); // no epoch_flags, no swarm
			const second = render(() => <StateExplainModal {...baseProps()} />);
			await waitFor(() => expect(second.container.textContent).toContain("Evidence"));
			expect(second.container.textContent).not.toContain("Declared background work");
			expect(second.container.textContent).not.toContain("Swarm");
		});

		it("renders the lead and each teammate, showing a closed terminal as closed", async () => {
			mockInvoke.mockResolvedValue(
				withDeclared(
					{},
					{
						lead_session_id: "leadleadlead",
						teammates: [
							{ session_id: "aaaa1111-x", shell_state: "busy", busy: true },
							{ session_id: "bbbb2222-x", shell_state: null, busy: false },
						],
					},
				),
			);
			const { container, getByText } = render(() => <StateExplainModal {...baseProps()} />);
			await waitFor(() => expect(getByText("Swarm")).not.toBeNull());
			const text = container.textContent ?? "";
			expect(text).toContain("leadlead");
			expect(text).toContain("teammate aaaa1111");
			expect(text).toContain("busy");
			expect(text).toContain("teammate bbbb2222");
			expect(text).toContain("closed");
		});
	});

	it("updates the mismatch banner when the badge inputs change while the modal is open", async () => {
		// Regression: `disagrees()` used to run once inside the render callback, so a
		// flag flipping after load updated the badge row but never the banner.
		const [shell, setShell] = createSignal<string | null>("busy");
		const [agent, setAgent] = createSignal<string | null>("working");
		const { container } = render(() => (
			<StateExplainModal {...baseProps()} shellState={shell()} agentState={agent()} />
		));
		await waitFor(() => expect(container.textContent).toContain("Evidence"));
		expect(container.textContent).not.toContain("disagrees with backend");

		setShell("idle");
		setAgent("idle");
		await waitFor(() => expect(container.textContent).toContain("disagrees with backend"));

		setShell("busy");
		setAgent("working");
		await waitFor(() => expect(container.textContent).not.toContain("disagrees with backend"));
	});
});
