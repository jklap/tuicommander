import { createEffect, createRoot, onCleanup } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { notificationsStore } from "../../stores/notifications";
import { terminalsStore } from "../../stores/terminals";

export const QUESTION_REMINDER_MS = 120_000;

/** Terminals whose reminder is already tracked: one tracker per id however many components mount it. */
const trackers = new Set<string>();

/**
 * Re-notify once when a terminal has been awaiting the same question for QUESTION_REMINDER_MS.
 * The tracker is owned by the terminal's id, not by the calling component, so a remount
 * keeps the original start time and two components mounting one id cannot double-fire.
 * It ends when the terminal leaves the store. Any change of awaitingInput or of the
 * question text (answer, agent resuming, error, a different question) restarts or
 * cancels the timer, so it fires at most once per question.
 * Idempotent: safe to call from every mount of the terminal.
 */
export function trackQuestionReminder(id: string): void {
	if (trackers.has(id)) return;
	trackers.add(id);
	createRoot((dispose) => {
		let timer: ReturnType<typeof setTimeout> | undefined;
		let key: string | null = null;
		let since = 0;
		const stop = () => {
			clearTimeout(timer);
			timer = undefined;
		};
		onCleanup(stop);
		createEffect(() => {
			const term = terminalsStore.get(id);
			stop();
			if (!term) {
				trackers.delete(id);
				dispose();
				return;
			}
			const pending = term.awaitingInput === "question" ? (term.awaitingInputText ?? "") : null;
			if (pending !== key) since = Date.now();
			key = pending;
			if (pending === null) return;
			timer = setTimeout(
				() => {
					timer = undefined;
					appLogger.info("terminal", `[Notify] ${id} question — still unanswered, reminding`);
					void notificationsStore.playQuestionReminder(id);
				},
				Math.max(0, since + QUESTION_REMINDER_MS - Date.now()),
			);
		});
	});
}
