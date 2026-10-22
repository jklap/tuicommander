import type { AutomationRun } from "../components/AutomationsDialog/contract";
import { automationsUi } from "../stores/automations";
import { toastsStore } from "../stores/toasts";

/** The backend owns durable per-channel deduplication. No boot/history replay. */
export function presentAutomationNotice(payload: unknown): void {
	const event = payload as { notice?: boolean; run?: AutomationRun };
	const run = event?.run;
	if (event?.notice !== true || !run || (run.status !== "needs_you" && run.status !== "failed")) return;
	toastsStore.addToBell(
		`${run.definition.name} · ${run.status}`,
		`Run ${run.id}${run.session_id ? ` · Session ${run.session_id}` : ""}`,
		run.status === "failed" ? "error" : "warn",
		run.definition.repository,
		{ label: "Open run", onClick: () => automationsUi.openRun(run.id) },
		run.session_id ?? undefined,
	);
}
