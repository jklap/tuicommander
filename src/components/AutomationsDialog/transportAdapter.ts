import { invoke } from "../../invoke";
import { rpc } from "../../transport";
import type {
	AutomationAction,
	AutomationAdapter,
	AutomationDefinition,
	AutomationListItem,
	AutomationRun,
	DefinitionSchedulePreview,
	RunReceipt,
	RunSummary,
	SchedulePreview,
} from "./contract";

/** Shared command envelope; invoke selects the addressed backend transport. */
export function createAutomationAdapter(connectionId?: string): AutomationAdapter {
	function call<T>(input: AutomationAction): Promise<T> {
		return connectionId
			? rpc<T>("automation_action", { input }, connectionId)
			: invoke<T>("automation_action", { input });
	}
	return {
		list: () => call<AutomationListItem[]>({ action: "list" }),
		save: (definition, create) => call<AutomationDefinition>({ action: create ? "create" : "update", definition }),
		setEnabled: (id, enabled) => call<void>({ action: enabled ? "resume" : "pause", id }),
		remove: (id) => call<void>({ action: "delete", id }),
		runNow: (id) => call<RunReceipt>({ action: "run_now", id }),
		history: (id, offset = 0, limit = 50) => call<AutomationRun[]>({ action: "list_runs", id, limit, offset }),
		summary: (window) => call<RunSummary>({ action: "summary", window }),
		preview: (cron, timezone) =>
			call<SchedulePreview>({ action: "preview", cron, timezone: timezone || null, count: 4 }),
		previewDefinition: (definition) =>
			call<DefinitionSchedulePreview>({ action: "preview_definition", definition, count: 4 }),
		preset: (preset) => call<{ cron: string }>({ action: "preset", preset }),
	};
}
export const automationAdapter = createAutomationAdapter();
