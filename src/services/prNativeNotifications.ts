import { notificationsStore } from "../stores/notifications";
import type { PrNotificationType } from "../stores/prNotifications";
import { showNativeNotice } from "./nativeNotifications";

/** Transitions that justify pulling Boss out of another app. */
const TITLES: Partial<Record<PrNotificationType, string>> = {
	ready: "PR ready to merge",
	ci_failed: "PR checks failed",
	changes_requested: "PR changes requested",
	merged: "PR merged",
};

/** The poller emits each transition once per observed change; the same event can still
 *  arrive twice (several event consumers, re-delivery). Identical events inside this
 *  window notify once. Genuine repeats (CI failing again after a fix push) are minutes apart. */
const DEDUP_WINDOW_MS = 120_000;
const lastSent = new Map<string, number>();

interface PrTransitionNotice {
	repoName: string;
	prNumber: number;
	title: string;
	type: PrNotificationType;
	url: string;
}

/** OS notification for a PR transition; clicking it opens the PR on GitHub. */
export function notifyPrTransition(n: PrTransitionNotice): void {
	const heading = TITLES[n.type];
	if (!heading || !notificationsStore.state.config.pr_native_notifications) return;

	// The URL names owner, repo and number; repoName alone collides across repos with the same folder name.
	const key = `pr:${n.url}:${n.type}`;
	const now = Date.now();
	if (now - (lastSent.get(key) ?? -Infinity) < DEDUP_WINDOW_MS) return;
	for (const [k, at] of lastSent) if (now - at >= DEDUP_WINDOW_MS) lastSent.delete(k);
	lastSent.set(key, now);

	void showNativeNotice({
		title: heading,
		body: `${n.repoName} #${n.prNumber}: ${n.title}`,
		key,
		target: { kind: "pr", url: n.url },
	});
}
