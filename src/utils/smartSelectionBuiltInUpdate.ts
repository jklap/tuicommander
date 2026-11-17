import { DEFAULT_SMART_SELECTION_RULES } from "../components/Terminal/smartSelectionDefaults";
import { reconcileBuiltInRules } from "../components/Terminal/smartSelectionReconcile";
import { settingsStore } from "../stores/settings";
import { toastsStore } from "../stores/toasts";

const TOAST_DURATION_MS = 15_000;

/**
 * After the settings are loaded: bring the user's saved smart-selection rules up to date with
 * whatever a new release changed in the shipped ones (`reconcileBuiltInRules`), persist the result
 * and, when something changed that the user should know about, say so once with a link straight to
 * the rules. Silent bookkeeping (stamping rules saved before versions existed) never toasts.
 *
 * Main window only: the panel windows and the mobile app also hydrate settings, and each running
 * this would race on the same file and toast twice.
 */
export function applyBuiltInRuleUpdates(openSettings: (tab?: string) => void): void {
	const result = reconcileBuiltInRules(settingsStore.state.smartSelectionRules, DEFAULT_SMART_SELECTION_RULES);
	if (!result.changed) return;
	settingsStore.setSmartSelectionRules(result.rules);
	if (result.updated.length === 0 && result.conflicts.length === 0) return;

	const lines: string[] = [];
	if (result.updated.length > 0) {
		lines.push(`Updated to the new built-in version: ${result.updated.join(", ")}.`);
	}
	if (result.conflicts.length > 0) {
		lines.push(
			`You had edited ${result.conflicts.join(", ")}, so your version is unchanged. ` +
				`The new built-in version was added next to it as a disabled "(updated)" rule for you to review.`,
		);
	}
	toastsStore.add(
		"Smart Selection rules updated",
		lines.join(" "),
		"info",
		false,
		{ label: "Go to the rules", onClick: () => openSettings("selection") },
		TOAST_DURATION_MS,
	);
}
