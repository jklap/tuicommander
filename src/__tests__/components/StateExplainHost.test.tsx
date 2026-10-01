import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

import { StateExplainHost } from "../../components/StateExplainModal/StateExplainHost";
import { stateExplainStore } from "../../stores/stateExplain";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";

describe("StateExplainHost", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue(null);
	});

	afterEach(() => {
		cleanup();
		stateExplainStore.close();
	});

	it("renders nothing while the terminal has no sessionId yet, then picks it up reactively once assigned — without reopening", async () => {
		// Code-review regression: the sessionId lookup used to run once inside
		// <Show>'s render-prop callback, which only re-invokes when
		// openTermId() itself changes — a terminal whose PTY session was
		// still spinning up stayed permanently blank.
		const id = terminalsStore.add(makeTerminal({ sessionId: null }));
		const { container } = render(() => <StateExplainHost />);

		stateExplainStore.open(id);
		await Promise.resolve();
		expect(container.querySelector(`.${"overlay"}`)).toBeNull();
		expect(mockInvoke).not.toHaveBeenCalled();

		terminalsStore.update(id, { sessionId: "session-abc" });

		await waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("explain_session_state", {
				sessionId: "session-abc",
			});
		});
	});

	// Backend fixture: agent_state "working" via the background_work rung, declared_background_work true.
	const explain = (over: Record<string, unknown>) => ({
		session_id: "session-decl",
		captured_at_ms: 1,
		agent: { agent_type: "claude", agent_seen_running: true, hook_instrumented: true, hook_state_seen: true },
		visible: {
			shell_state: "idle",
			agent_state: "working",
			agent_state_rung: "background_work",
			awaiting_input: false,
			question_confident: false,
			choice_prompt_present: false,
			background_work: false,
			declared_background_work: true,
			rate_limited: false,
			queued_commands: 0,
			turn_epoch: 1,
			active_sub_tasks: 0,
			...over,
		},
		evidence: {
			busy: null,
			idle: null,
			awaiting: null,
			activity_seen: false,
			idle_confirmed: false,
			shell_is_busy: false,
			decide_now: null,
		},
		epoch_flags: {},
		screen: {
			cached_activity: "ready",
			screen_ready_pending_since_ms: null,
			skipped_by_protocol_authority: false,
			no_adapter_for_agent: false,
		},
		silence: {},
		holds: {},
		notification: null,
		trail: [],
	});

	it("passes the terminal's declaredBackgroundWork through to the modal's frontend badge (agrees with backend)", async () => {
		mockInvoke.mockResolvedValue(explain({}));
		const id = terminalsStore.add(makeTerminal({ sessionId: "session-decl" }));
		terminalsStore.update(id, { shellState: "idle", agentState: "idle", declaredBackgroundWork: true });
		const { container } = render(() => <StateExplainHost />);
		stateExplainStore.open(id);

		await waitFor(() => {
			expect(container.textContent).toContain("frontend badgeworking");
		});
		expect(container.textContent).not.toContain("disagrees with backend");
	});

	it("without declaredBackgroundWork the same idle terminal reads idle and the disagreement banner appears", async () => {
		mockInvoke.mockResolvedValue(explain({}));
		const id = terminalsStore.add(makeTerminal({ sessionId: "session-decl" }));
		terminalsStore.update(id, { shellState: "idle", agentState: "idle", declaredBackgroundWork: false });
		const { container } = render(() => <StateExplainHost />);
		stateExplainStore.open(id);

		await waitFor(() => {
			expect(container.textContent).toContain("frontend badgeidle");
		});
		expect(container.textContent).toContain("disagrees with backend");
	});

	it("renders nothing when no terminal is open", () => {
		const { container } = render(() => <StateExplainHost />);
		expect(container.textContent).toBe("");
	});
});
