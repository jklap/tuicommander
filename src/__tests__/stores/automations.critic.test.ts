import { expect, it } from "vitest";
import type { AutomationAdapter, AutomationDefinition } from "../../components/AutomationsDialog/contract";
import { createAutomationsStore } from "../../stores/automations";

it("pausing does not overwrite a newer prompt saved by an agent", async () => {
	let persisted: AutomationDefinition = {
		id: "morning",
		name: "Morning",
		prompt: "Original prompt",
		repository: "/repo",
		run_config: "codex",
		workspace: { mode: "existing" },
		cron: "0 9 * * *",
		timezone: "Europe/Madrid",
		enabled: true,
		grace_secs: 43200,
		overlap: "skip",
		max_duration_secs: 3600,
		precheck: null,
	};
	const adapter: AutomationAdapter = {
		list: async () => [{ definition: structuredClone(persisted), next_run_ms: null, last_status: null }],
		save: async (definition) => {
			persisted = structuredClone(definition);
			return structuredClone(persisted);
		},
		remove: async () => {},
		runNow: async () => ({ status: "reserved", reason: null }),
		history: async () => [],
		preview: async (cron, timezone) => ({ cron, timezone, occurrences: [] }),
		preset: async () => ({ cron: "0 9 * * *" }),
	};
	const store = createAutomationsStore(adapter);
	await store.refresh();
	store.select(store.items()[0].definition);
	// MCP and the dialog share the persisted definition; the dialog has an older list snapshot.
	persisted.prompt = "Agent updated instructions";
	await store.setEnabled(false);
	expect(persisted.enabled).toBe(false);
	expect(persisted.prompt).toBe("Agent updated instructions");
});
