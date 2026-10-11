import { beforeEach, describe, expect, it } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";
import { render, waitFor } from "@solidjs/testing-library";
import { PrActionButtons } from "../../components/PrDetailPopover/PrActionButtons";
import type { BranchPrStatus } from "../../types";

const pr = {
	number: 7,
	state: "OPEN",
	is_draft: false,
	head_ref_oid: "abc123",
	merge_state_status: "CLEAN",
} as unknown as BranchPrStatus;

/** Serialized shape of Rust `PrActionState` (`#[serde(rename_all = "camelCase")]`, github_pr_actions.rs). */
const wire = (over: Record<string, unknown>) => ({
	id: "PR_1",
	headSha: "abc123",
	baseRefName: "main",
	state: "OPEN",
	isDraft: false,
	autoMergeRequest: null,
	mergeQueueEntry: null,
	queueRequired: false,
	...over,
});

describe("PrActionButtons against the real backend wire shape", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("shows Disable auto-merge when the backend reports autoMergeRequest — catches: reading snake_case keys the backend never sends", async () => {
		mockInvoke.mockResolvedValue(wire({ autoMergeRequest: { enabledAt: "2026-10-11" } }));
		const { findByText } = render(() => <PrActionButtons repoPath="/repo" pr={pr} checks={[]} />);
		expect(await findByText("Disable auto-merge")).toBeTruthy();
	});

	it("offers the merge queue when the backend reports queueRequired — catches: queue button unreachable", async () => {
		mockInvoke.mockResolvedValue(wire({ queueRequired: true }));
		const { findByText } = render(() => <PrActionButtons repoPath="/repo" pr={pr} checks={[]} />);
		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_pr_action_state", expect.anything()));
		expect(await findByText("Add to merge queue")).toBeTruthy();
	});

	it("offers Remove from merge queue when the backend reports mergeQueueEntry — catches: cannot leave a queue", async () => {
		mockInvoke.mockResolvedValue(wire({ mergeQueueEntry: { position: 1 } }));
		const { findByText } = render(() => <PrActionButtons repoPath="/repo" pr={pr} checks={[]} />);
		expect(await findByText("Remove from merge queue")).toBeTruthy();
	});
});
