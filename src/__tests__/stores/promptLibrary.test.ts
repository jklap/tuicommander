import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("../../invoke", () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
	listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../../transport", async (importOriginal) => {
	const orig = await importOriginal<typeof import("../../transport")>();
	return { ...orig, isTauri: () => true };
});

describe("promptLibraryStore", () => {
	let store: typeof import("../../stores/promptLibrary").promptLibraryStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		localStorage.clear();

		vi.doMock("../../invoke", () => ({
			invoke: (...args: unknown[]) => mockInvoke(...args),
			listen: vi.fn().mockResolvedValue(() => {}),
		}));
		vi.doMock("../../transport", async (importOriginal) => {
			const orig = await importOriginal<typeof import("../../transport")>();
			return { ...orig, isTauri: () => true };
		});

		store = (await import("../../stores/promptLibrary")).promptLibraryStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	describe("createPrompt()", () => {
		it("creates a prompt with generated ID", () => {
			testInScope(() => {
				const prompt = store.createPrompt({
					name: "Test Prompt",
					content: "Hello {name}!",
					category: "custom",
					isFavorite: false,
				});
				expect(prompt.id).toBeTruthy();
				expect(prompt.name).toBe("Test Prompt");
				expect(prompt.createdAt).toBeGreaterThan(0);
			});
		});

		it("persists via invoke", () => {
			testInScope(() => {
				store.createPrompt({
					name: "Test",
					content: "content",
					category: "custom",
					isFavorite: false,
				});
				vi.advanceTimersByTime(600); // flush debounced save
				expect(mockInvoke).toHaveBeenCalledWith("save_prompt_library", {
					config: expect.objectContaining({
						prompts: expect.arrayContaining([
							expect.objectContaining({ label: "Test", text: expect.stringContaining('"content":"content"') }),
						]),
					}),
				});
			});
		});
	});

	describe("importPrompts()", () => {
		it("upserts all prompts in a single save", () => {
			testInScope(() => {
				const result = store.importPrompts([
					{
						id: "p1",
						name: "P1",
						content: "c1",
						category: "custom",
						isFavorite: false,
						createdAt: 1,
						updatedAt: 1,
					},
					{
						id: "p2",
						name: "P2",
						content: "c2",
						category: "custom",
						isFavorite: false,
						createdAt: 1,
						updatedAt: 1,
					},
				]);
				expect(result.imported).toBe(2);
				expect(result.disabled).toEqual([]);
				expect(store.getPrompt("p1")?.name).toBe("P1");
				expect(store.getPrompt("p2")?.name).toBe("P2");

				vi.advanceTimersByTime(600);
				// One debounced save for the whole batch, not one per prompt
				expect(mockInvoke).toHaveBeenCalledTimes(1);
			});
		});

		it("imports shell/api prompts disabled and reports them", () => {
			testInScope(() => {
				const result = store.importPrompts([
					{
						id: "p1",
						name: "Prune Branches",
						content: "git branch -d $(git branch --merged)",
						category: "custom",
						isFavorite: false,
						createdAt: 1,
						updatedAt: 1,
						executionMode: "shell",
						enabled: true,
					},
				]);
				expect(result.disabled).toEqual(["Prune Branches"]);
				expect(store.getPrompt("p1")?.enabled).toBe(false);
			});
		});

		it("preserves createdAt on a conflicting overwrite", () => {
			testInScope(() => {
				const original = store.createPrompt({ name: "Original", content: "c", category: "custom", isFavorite: false });
				store.importPrompts([
					{
						...original,
						name: "Overwritten",
						content: "new content",
						createdAt: 999999,
					},
				]);
				const updated = store.getPrompt(original.id);
				expect(updated?.name).toBe("Overwritten");
				expect(updated?.createdAt).toBe(original.createdAt);
			});
		});

		it("clears a forged builtIn flag on an id that isn't an actual built-in", () => {
			testInScope(() => {
				store.importPrompts([
					{
						id: "not-a-real-builtin",
						name: "Fake",
						content: "c",
						category: "custom",
						isFavorite: false,
						createdAt: 1,
						updatedAt: 1,
						builtIn: true,
					},
				]);
				expect(store.getPrompt("not-a-real-builtin")?.builtIn).toBe(false);
			});
		});
	});

	describe("updatePrompt()", () => {
		it("updates existing prompt", () => {
			testInScope(() => {
				const prompt = store.createPrompt({
					name: "Old Name",
					content: "content",
					category: "custom",
					isFavorite: false,
				});
				store.updatePrompt(prompt.id, { name: "New Name" });
				expect(store.getPrompt(prompt.id)?.name).toBe("New Name");
			});
		});

		it("ignores updates for non-existent prompts", () => {
			testInScope(() => {
				store.updatePrompt("nonexistent", { name: "Updated" }); // Should not throw
			});
		});
	});

	describe("deletePrompt()", () => {
		it("deletes a prompt", () => {
			testInScope(() => {
				const prompt = store.createPrompt({
					name: "Test",
					content: "content",
					category: "custom",
					isFavorite: false,
				});
				store.deletePrompt(prompt.id);
				expect(store.getPrompt(prompt.id)).toBeUndefined();
			});
		});
	});

	describe("toggleFavorite()", () => {
		it("toggles favorite status", () => {
			testInScope(() => {
				const prompt = store.createPrompt({
					name: "Test",
					content: "content",
					category: "custom",
					isFavorite: false,
				});
				store.toggleFavorite(prompt.id);
				expect(store.getPrompt(prompt.id)?.isFavorite).toBe(true);
				store.toggleFavorite(prompt.id);
				expect(store.getPrompt(prompt.id)?.isFavorite).toBe(false);
			});
		});
	});

	describe("markAsUsed()", () => {
		it("updates lastUsed timestamp", () => {
			testInScope(() => {
				const prompt = store.createPrompt({
					name: "Test",
					content: "content",
					category: "custom",
					isFavorite: false,
				});
				store.markAsUsed(prompt.id);
				expect(store.getPrompt(prompt.id)?.lastUsed).toBeGreaterThan(0);
			});
		});

		it("adds to recent list", () => {
			testInScope(() => {
				const prompt = store.createPrompt({
					name: "Test",
					content: "content",
					category: "custom",
					isFavorite: false,
				});
				store.markAsUsed(prompt.id);
				expect(store.state.recentIds).toContain(prompt.id);
			});
		});
	});

	describe("getAllPrompts()", () => {
		it("returns all prompts as array", () => {
			testInScope(() => {
				store.createPrompt({ name: "P1", content: "c1", category: "custom", isFavorite: false });
				store.createPrompt({ name: "P2", content: "c2", category: "custom", isFavorite: false });
				expect(store.getAllPrompts()).toHaveLength(2);
			});
		});
	});

	describe("hydrate()", () => {
		it("loads prompts from Rust backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				prompts: [{ id: "p1", label: "Test", text: "content", pinned: false }],
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.getPrompt("p1")?.name).toBe("Test");
				expect(store.getPrompt("p1")?.content).toBe("content");
				expect(mockInvoke).toHaveBeenCalledWith("load_prompt_library");
			});
		});

		it("handles backend failure gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("backend unavailable"));

			await testInScopeAsync(async () => {
				await store.hydrate();
				// Store should remain usable with empty state after failure
				expect(store.getAllPrompts()).toEqual([]);
			});
		});

		it("migrates from localStorage on first run", async () => {
			const legacyPrompts = {
				p1: {
					id: "p1",
					name: "Legacy",
					content: "old",
					category: "custom",
					isFavorite: true,
					createdAt: 1,
					updatedAt: 1,
				},
			};
			localStorage.setItem("tui-commander-prompt-library", JSON.stringify(legacyPrompts));
			mockInvoke.mockResolvedValueOnce(undefined); // save_prompt_library migration
			mockInvoke.mockResolvedValueOnce({ prompts: [] }); // load_prompt_library

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-prompt-library")).toBeNull();
			});
		});

		it("loads a full SavedPrompt from valid stored JSON (not just the legacy simple-format fallback)", async () => {
			const stored = {
				id: "p1",
				name: "Full Prompt",
				content: "do the thing",
				category: "custom" as const,
				isFavorite: true,
				createdAt: 5,
				updatedAt: 5,
				executionMode: "headless" as const,
				placement: ["toolbar" as const],
			};
			mockInvoke.mockResolvedValueOnce({
				prompts: [{ id: "p1", label: "Full Prompt", text: JSON.stringify(stored), pinned: true }],
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				const loaded = store.getPrompt("p1");
				expect(loaded?.name).toBe("Full Prompt");
				expect(loaded?.executionMode).toBe("headless");
				expect(loaded?.placement).toEqual(["toolbar"]);
			});
		});

		it("forwards sanitizePrompt warnings through appLogger during hydrate", async () => {
			const { appLogger } = await import("../../stores/appLogger");
			const warnSpy = vi.spyOn(appLogger, "warn");
			const stored = {
				id: "p1",
				name: "Bad Mode",
				content: "c",
				category: "custom" as const,
				isFavorite: false,
				createdAt: 1,
				updatedAt: 1,
				// biome-ignore lint/suspicious/noExplicitAny: deliberately invalid input under test
				executionMode: "delete-everything" as any,
			};
			mockInvoke.mockResolvedValueOnce({
				prompts: [{ id: "p1", label: "Bad Mode", text: JSON.stringify(stored), pinned: false }],
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.getPrompt("p1")?.executionMode).toBe("inject");
				expect(warnSpy.mock.calls.some((call) => String(call[1]).includes("invalid executionMode"))).toBe(true);
			});
		});

		it("re-saves after migrating a legacy tab-context placement during hydrate", async () => {
			const stored = {
				id: "p1",
				name: "Legacy Placement",
				content: "c",
				category: "custom" as const,
				isFavorite: false,
				createdAt: 1,
				updatedAt: 1,
				// biome-ignore lint/suspicious/noExplicitAny: exercising the legacy placement name
				placement: ["tab-context"] as any,
			};
			mockInvoke.mockResolvedValueOnce({
				prompts: [{ id: "p1", label: "Legacy Placement", text: JSON.stringify(stored), pinned: false }],
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.getPrompt("p1")?.placement).toEqual(["terminal-context"]);
				vi.advanceTimersByTime(600);
				expect(mockInvoke).toHaveBeenCalledWith("save_prompt_library", expect.anything());
			});
		});
	});
});
