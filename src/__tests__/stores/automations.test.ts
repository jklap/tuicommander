import { createRoot } from "solid-js";
import { describe, expect, it } from "vitest";
import type { AutomationDefinition, SchedulePreview } from "../../components/AutomationsDialog/contract";
import { createAutomationsStore } from "../../stores/automations";
import { definition, fakeAdapter } from "../automationsFixtures";

describe("automations dialog state", () => {
	it("retains the editable prompt after a rejected save instead of losing unsaved work", async () => {
		await createRoot(async (dispose) => {
			const store = createAutomationsStore(
				fakeAdapter({
					save: async () => {
						throw new Error("Invalid run config");
					},
				}),
			);
			await store.refresh();
			store.select(definition);
			store.edit({ prompt: "Keep this draft" });
			await store.save();
			expect(store.draft()?.prompt).toBe("Keep this draft");
			expect(store.error()).toBe("Invalid run config");
			dispose();
		});
	});
	it("ignores an older preview that would validate a different cron than the current edit", async () => {
		await createRoot(async (dispose) => {
			let resolveOld: ((value: SchedulePreview) => void) | undefined;
			const store = createAutomationsStore(
				fakeAdapter({
					preview: async (cron, timezone) =>
						cron === "old"
							? new Promise((resolve) => {
									resolveOld = resolve;
								})
							: { cron, timezone, occurrences: [] },
				}),
			);
			store.select(definition);
			store.edit({ cron: "old" });
			const older = store.preview();
			store.edit({ cron: "new" });
			await store.preview();
			resolveOld?.({ cron: "old", timezone: "Europe/Madrid", occurrences: ["wrong"] });
			await older;
			expect(store.schedulePreview()?.cron).toBe("new");
			dispose();
		});
	});
	it("reports capacity refusal honestly when Run Now was admitted as a skipped run", async () => {
		await createRoot(async (dispose) => {
			const store = createAutomationsStore(fakeAdapter());
			store.select({ ...definition, enabled: false });
			await store.runNow();
			expect(store.notice()).toContain("All slots are busy");
			expect(store.notice()).toContain("skipped_concurrency");
			dispose();
		});
	});
	it("pauses the saved definition without accidentally saving a changed draft prompt", async () => {
		await createRoot(async (dispose) => {
			let saved: AutomationDefinition | undefined;
			const store = createAutomationsStore(
				fakeAdapter({
					save: async (value) => {
						saved = value;
						return value;
					},
				}),
			);
			await store.refresh();
			store.select(definition);
			store.edit({ prompt: "Unsaved" });
			await store.setEnabled(false);
			expect(saved?.prompt).toBe(definition.prompt);
			expect(saved?.enabled).toBe(false);
			expect(store.draft()?.prompt).toBe("Unsaved");
			dispose();
		});
	});
	it("does not replace the selected history when an earlier request finishes late", async () => {
		await createRoot(async (dispose) => {
			let finish: ((value: never[]) => void) | undefined;
			const store = createAutomationsStore(
				fakeAdapter({
					history: async (id) =>
						id === "a1"
							? new Promise((resolve) => {
									finish = resolve;
								})
							: [],
				}),
			);
			store.select(definition);
			const old = store.loadHistory();
			store.select({ ...definition, id: "a2" });
			await store.loadHistory();
			finish?.([]);
			await old;
			expect(store.draft()?.id).toBe("a2");
			expect(store.runs()).toEqual([]);
			dispose();
		});
	});
});
