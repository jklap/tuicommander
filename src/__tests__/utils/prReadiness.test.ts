import { describe, expect, it } from "vitest";
import type { BranchPrStatus } from "../../types";
import { PR_READINESS_LABELS, PR_READINESS_SEVERITY, prReadiness, prReadinessOf } from "../../utils/prReadiness";

const open = { state: "OPEN", conflictState: "clear", mergeable: "MERGEABLE", reviewDecision: "APPROVED" };

describe("prReadiness", () => {
	it("never reports ready or ok while CI is red", () => {
		// The Ops dashboard used to ignore review state and the badge ignored nothing:
		// an approved, mergeable PR with a failing check must read as blocked everywhere.
		const kind = prReadiness({ ...open, ciFailed: 1 });
		expect(kind).toBe("ci-failed");
		expect(PR_READINESS_SEVERITY[kind]).toBe("critical");
	});

	it("treats an unknown conflict state as pending, not as green", () => {
		// GitHub still recomputing mergeable after a push: UNKNOWN must not read as Ready.
		expect(prReadiness({ ...open, conflictState: "checking" })).toBe("checking");
	});

	it("blocks readiness on unresolved threads even when approved and green", () => {
		const kind = prReadiness({ ...open, unresolvedThreads: 2 });
		expect(kind).toBe("unresolved-comments");
		expect(PR_READINESS_LABELS[kind]).toBe("Comments");
		expect(PR_READINESS_SEVERITY[kind]).toBe("warn");
	});

	it("ranks red CI and requested changes above unresolved threads", () => {
		expect(prReadiness({ ...open, ciFailed: 1, unresolvedThreads: 3 })).toBe("ci-failed");
		expect(prReadiness({ ...open, reviewDecision: "CHANGES_REQUESTED", unresolvedThreads: 3 })).toBe(
			"changes-requested",
		);
	});

	it("ignores threads on a merged, closed or draft PR", () => {
		expect(prReadiness({ ...open, state: "MERGED", unresolvedThreads: 4 })).toBe("merged");
		expect(prReadiness({ ...open, state: "CLOSED", unresolvedThreads: 4 })).toBe("closed");
		expect(prReadiness({ ...open, isDraft: true, unresolvedThreads: 4 })).toBe("draft");
	});

	it("is ready only when approved, mergeable, green and thread-free", () => {
		expect(prReadiness({ ...open, unresolvedThreads: 0 })).toBe("ready");
		expect(prReadiness({ ...open, ciPending: 1 })).toBe("ci-pending");
	});

	it("gives the badge adapter the same verdict as the raw input", () => {
		// Panel and Ops go through prReadinessOf; a drift between the two shapes would
		// reintroduce the three-surfaces-three-answers bug.
		const pr = {
			state: "OPEN",
			is_draft: false,
			conflict_state: "clear",
			mergeable: "MERGEABLE",
			review_decision: "APPROVED",
			checks: { passed: 1, failed: 0, pending: 0, total: 1 },
			unresolved_threads: 1,
		} as BranchPrStatus;
		expect(prReadinessOf(pr)).toBe("unresolved-comments");
	});
});
