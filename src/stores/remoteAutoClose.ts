import type { AgentType } from "../agents";
import { isSuspendingOrSuspended } from "../utils/suspendTerminal";
import { appLogger } from "./appLogger";
import { terminalsStore } from "./terminals";

/** Delay before auto-removing a remote tab after its session exits. Gives the
 *  user time to see "[Process exited]" in the terminal before it vanishes. */
export const REMOTE_TAB_AUTOCLOSE_MS = 30_000;
/** Shorter delay for agent-spawned sessions — they finish their task and can
 *  be cleaned up faster than manually-opened remote sessions. */
export const AGENT_TAB_AUTOCLOSE_MS = 10_000;

/** termId -> live countdown ticker. The one piece of state that makes
 *  `scheduleRemoteAutoClose` idempotent and lets `resumeStrandedAutoCloseTabs`
 *  tell "already counting down" apart from "stuck with none running" without
 *  touching `terminalsStore` itself (a persisted `autoCloseAt` field would
 *  survive this map's loss across a reload, but not the loss of the thing
 *  that would act on it — nothing drives a deadline while the page is gone —
 *  so a sweep that just starts a fresh countdown on next init covers the same
 *  cases without a persistence format to maintain). */
const activeCountdowns = new Map<string, ReturnType<typeof setInterval>>();

terminalsStore.onRemove((termId) => {
	const ticker = activeCountdowns.get(termId);
	if (ticker) {
		clearInterval(ticker);
		activeCountdowns.delete(termId);
	}
});

/**
 * Start the auto-close countdown for an exited remote tab. No-ops if the tab
 * already has a countdown running, isn't remote, hasn't exited, or is suspended
 * (a suspend ends the PTY on purpose and the tab must stay, restorable — the
 * same rule `session-closed` applies) — so it is safe to call from any
 * exit-detection path, not just the one that happens to run first.
 *
 * `agentTypeHint`: pass the value a caller already has on hand (e.g. the
 * backend `session-closed` event's own `agent_type`) when the store's
 * `agentType` field may already have been cleared by that same caller's own
 * teardown. Omit it to read the store's current value instead — correct for
 * any caller that hasn't touched `agentType` itself (the lifecycle-polling
 * catch-up, and the stranded-tab sweep below).
 */
export function scheduleRemoteAutoClose(termId: string, agentTypeHint?: AgentType | null): void {
	if (activeCountdowns.has(termId)) return;
	const t0 = terminalsStore.get(termId);
	if (!t0?.isRemote || t0.shellState !== "exited") return;
	if (isSuspendingOrSuspended(termId)) return;

	const effectiveAgentType = agentTypeHint !== undefined ? agentTypeHint : t0.agentType;
	const autoCloseMs = effectiveAgentType != null ? AGENT_TAB_AUTOCLOSE_MS : REMOTE_TAB_AUTOCLOSE_MS;
	appLogger.info("app", `Remote tab ${termId} exited — auto-close in ${autoCloseMs}ms`);

	// Countdown in the tab name so the user sees when it will vanish. `{ echo:
	// false }`: this is cosmetic-only display text, never the session's real
	// display name.
	const baseName = t0.name ?? termId;
	let remaining = Math.round(autoCloseMs / 1000);
	// The label this countdown last wrote. Any other name on a later tick means
	// the user (or the backend) renamed the tab mid-countdown: stop writing the
	// label from then on, so the rename survives.
	let ownLabel: string | null = `${baseName} (${remaining}s)`;
	terminalsStore.update(termId, { name: ownLabel }, { echo: false });

	const stop = () => {
		clearInterval(ticker);
		activeCountdowns.delete(termId);
	};
	const ticker = setInterval(() => {
		remaining--;
		const t = terminalsStore.get(termId);
		if (ownLabel !== null && t?.name !== ownLabel) ownLabel = null;
		// Re-check every tick, not just at the start: the tab may have been
		// suspended, or its session revived (no longer exited), since the
		// countdown began — neither may be auto-closed.
		if (t?.isRemote && (t.shellState !== "exited" || isSuspendingOrSuspended(termId))) {
			stop();
			appLogger.info("app", `Remote tab ${termId} no longer exited/closable — auto-close cancelled`);
			if (ownLabel !== null) terminalsStore.update(termId, { name: baseName }, { echo: false });
			return;
		}
		if (!t?.isRemote || remaining <= 0) {
			stop();
			if (t?.isRemote) {
				appLogger.info("app", `Auto-removing remote tab ${termId} (countdown elapsed)`);
				terminalsStore.remove(termId);
			}
			return;
		}
		if (ownLabel !== null) {
			ownLabel = `${baseName} (${remaining}s)`;
			terminalsStore.update(termId, { name: ownLabel }, { echo: false });
		}
	}, 1000);
	activeCountdowns.set(termId, ticker);
}

/**
 * Self-heal sweep: start a countdown for every exited remote tab that doesn't
 * already have one running — `scheduleRemoteAutoClose`'s own guards make this
 * a no-op for everything else. Covers two independent ways a tab can end up
 * permanently stuck "exited, remote, no countdown, never removed" with a
 * clean name (no stuck "(Ns)" suffix): an exit-detection path that marks
 * `shellState: "exited"` without calling `scheduleRemoteAutoClose` at all
 * (`useAgentPolling.ts`'s stale-session catch-up used to be exactly this), and
 * a countdown whose `setInterval` lived only in this module's in-memory map —
 * a full frontend reload or HMR of this module wipes it mid-flight, and since
 * the backend's `session-closed` is a one-shot push that already fired, that
 * tab can never get a second countdown unless something resweeps it.
 *
 * Call once at app init (after sessions are hydrated) and anywhere else a
 * batch of sessions gets reconciled against the backend's live list.
 */
export function resumeStrandedAutoCloseTabs(): void {
	for (const termId of terminalsStore.getIds()) {
		scheduleRemoteAutoClose(termId);
	}
}
