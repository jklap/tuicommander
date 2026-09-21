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

	it("renders nothing when no terminal is open", () => {
		const { container } = render(() => <StateExplainHost />);
		expect(container.textContent).toBe("");
	});
});
