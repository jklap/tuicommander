import type { Component } from "solid-js";
import { cx } from "../../utils";
import s from "./Sidebar.module.css";

interface PrBadgeState {
	state?: string;
	isDraft?: boolean;
	mergeable?: string;
	conflictState?: string;
	reviewDecision?: string;
	ciFailed?: number;
	ciPending?: number;
}

const PR_BADGE_CLASSES: Record<string, string> = {
	ready: s.prReady,
	open: s.prOpen,
	merged: s.prMerged,
	closed: s.prClosed,
	draft: s.prDraft,
	conflict: s.prConflict,
	checking: s.prCiPending,
	"ci-failed": s.prCiFailed,
	"changes-requested": s.prChangesRequested,
	"review-required": s.prReviewRequired,
	"ci-pending": s.prCiPending,
};

function prBadgeKind(props: PrBadgeState): string {
	if (props.isDraft) return "draft";
	const state = props.state?.toLowerCase();
	if (state === "merged") return "merged";
	if (state === "closed") return "closed";
	if (props.conflictState === "conflicting") return "conflict";
	if (props.conflictState === "checking") return "checking";
	if ((props.ciFailed ?? 0) > 0) return "ci-failed";
	if (props.reviewDecision === "CHANGES_REQUESTED") return "changes-requested";
	if (props.reviewDecision === "REVIEW_REQUIRED") return "review-required";
	if ((props.ciPending ?? 0) > 0) return "ci-pending";
	if (props.mergeable === "MERGEABLE" && props.reviewDecision === "APPROVED") return "ready";
	return "open";
}

export function isPrBadgeFlashing(props: PrBadgeState): boolean {
	return ["conflict", "checking", "ci-pending"].includes(prBadgeKind(props));
}

/** PR state badge — keeps the PR identity visible alongside its highest-priority state. */
export const PrStateBadge: Component<{
	prNumber: number;
	state?: string;
	isDraft?: boolean;
	mergeable?: string;
	/** Backend verdict — the ONE rule (`classify_conflict_state`). Never re-derive
	 *  a conflict from `mergeable` here: GitHub keeps serving the last known value
	 *  while it recomputes, which is how this badge used to accuse a PR of
	 *  conflicting on data GitHub had already invalidated (#8537). */
	conflictState?: string;
	reviewDecision?: string;
	ciPassed?: number;
	ciFailed?: number;
	ciPending?: number;
	/** Files a removal would discard. This badge takes the row's one chip slot,
	 *  so it carries the warning the lifecycle chip would have shown. */
	dirtyFiles?: number;
}> = (props) => {
	const badge = (): { label: string; cls: string } => {
		const withNumber = (state: string) => `#${props.prNumber} ${state}`;
		const cls = prBadgeKind(props);
		const labels: Record<string, string> = {
			draft: "Draft",
			merged: "Merged",
			closed: "Closed",
			conflict: "Conflicts",
			checking: "Checking",
			"ci-failed": "CI Failed",
			"changes-requested": "Changes Req.",
			"review-required": "Review",
			"ci-pending": "CI Running",
			ready: "Ready",
		};
		return { label: labels[cls] ? withNumber(labels[cls]) : `#${props.prNumber}`, cls };
	};

	return (
		<span
			class={cx(s.prBadge, PR_BADGE_CLASSES[badge().cls])}
			title={`PR #${props.prNumber}${
				props.dirtyFiles ? ` — ${props.dirtyFiles} uncommitted file${props.dirtyFiles === 1 ? "" : "s"}` : ""
			}`}
		>
			{badge().label}
		</span>
	);
};
