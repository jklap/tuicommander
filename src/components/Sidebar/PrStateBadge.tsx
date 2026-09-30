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

const PR_STATE_LABELS: Record<string, string> = {
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

/** Compact form: the state is carried by the marker's color (and, for a
 *  conflict, its shape) instead of a filled pill. */
const PR_MARK_CLASSES: Record<string, string> = {
	ready: s.prMarkOpen,
	open: s.prMarkOpen,
	merged: s.prMarkMerged,
	closed: s.prMarkClosed,
	draft: s.prMarkDraft,
	conflict: s.prMarkConflict,
	checking: s.prMarkPending,
	"ci-failed": s.prMarkClosed,
	"changes-requested": s.prMarkChanges,
	"review-required": s.prMarkReview,
	"ci-pending": s.prMarkPending,
};

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
	/** Branch-row form: a colored marker and the number, the state word moved
	 *  into a `data-tooltip` (WKWebView never renders a native `title`). */
	compact?: boolean;
}> = (props) => {
	const dirtySuffix = () =>
		props.dirtyFiles ? ` — ${props.dirtyFiles} uncommitted file${props.dirtyFiles === 1 ? "" : "s"}` : "";
	const badge = (): { label: string; cls: string } => {
		const withNumber = (state: string) => `#${props.prNumber} ${state}`;
		const cls = prBadgeKind(props);
		const label = PR_STATE_LABELS[cls];
		return { label: label ? withNumber(label) : `#${props.prNumber}`, cls };
	};

	if (props.compact) {
		const tooltip = () => {
			const label = PR_STATE_LABELS[badge().cls];
			return `PR #${props.prNumber}${label ? ` · ${label}` : ""}${dirtySuffix()}`;
		};
		return (
			<span
				class={cx(s.prBadge, s.prBadgeCompact, PR_MARK_CLASSES[badge().cls])}
				data-tooltip={tooltip()}
				data-tooltip-pos="bottom"
				data-tooltip-align="right"
				aria-label={tooltip()}
			>
				<span class={s.prMark} aria-hidden="true" />#{props.prNumber}
			</span>
		);
	}

	return (
		<span class={cx(s.prBadge, PR_BADGE_CLASSES[badge().cls])} title={`PR #${props.prNumber}${dirtySuffix()}`}>
			{badge().label}
		</span>
	);
};
