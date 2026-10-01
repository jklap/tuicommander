import { createEffect, onCleanup } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { notificationsStore } from "../../stores/notifications";
import { terminalsStore } from "../../stores/terminals";

export const QUESTION_REMINDER_MS = 120_000;

/**
 * Re-notify once when a terminal has been awaiting a question for QUESTION_REMINDER_MS.
 * Any change of awaitingInput (answer, agent resuming, error, a new question after a
 * clear) or removing the terminal restarts or cancels the timer, so it fires at most
 * once per question. Sound and OS notice follow the playQuestion settings.
 * Call inside a component/root owner.
 */
export function trackQuestionReminder(id: string): void {
	let timer: ReturnType<typeof setTimeout> | undefined;
	const cancel = () => {
		clearTimeout(timer);
		timer = undefined;
	};
	createEffect(() => {
		cancel();
		if (terminalsStore.get(id)?.awaitingInput !== "question") return;
		timer = setTimeout(() => {
			timer = undefined;
			if (terminalsStore.get(id)?.awaitingInput !== "question") return;
			appLogger.info("terminal", `[Notify] ${id} question — still unanswered, reminding`);
			void notificationsStore.playQuestion(id);
		}, QUESTION_REMINDER_MS);
	});
	onCleanup(cancel);
}
