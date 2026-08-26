import { createEffect, createRoot } from "solid-js";
import { createStore } from "solid-js/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

/** Most recent invoke() call for `cmd` — targets ES2021 lib, so no Array.findLast. */
function lastInvokeCall(cmd: string): unknown[] | undefined {
	const calls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === cmd);
	return calls[calls.length - 1];
}

/** Shape of one entry as posted/read over the `save_repo_settings`/`load_repo_settings`
 *  wire boundary — snake_case, values loosely typed since a test only cares about a few
 *  fields at a time and the point of these assertions is to catch key-name drift. */
interface WireRepoEntry {
	[key: string]: string | number | boolean | string[] | null | undefined;
}

// Mock repoDefaultsStore so getEffective tests are deterministic. It is a real
// Solid store, not a plain object, so tests can observe which reads actually
// subscribe to it.
const [mockDefaults, setMockDefaults] = createStore({
	baseBranch: "automatic",
	copyIgnoredFiles: false,
	copyUntrackedFiles: false,
	setupScript: "",
	runScript: "",
	archiveScript: "",
});

vi.mock("../../stores/repoDefaults", () => ({
	repoDefaultsStore: { state: mockDefaults },
}));

describe("repoSettingsStore", () => {
	let store: typeof import("../../stores/repoSettings").repoSettingsStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		localStorage.clear();

		// Reset mock defaults to known state
		setMockDefaults({
			baseBranch: "automatic",
			copyIgnoredFiles: false,
			copyUntrackedFiles: false,
			setupScript: "",
			runScript: "",
			archiveScript: "",
		});

		vi.doMock("@tauri-apps/api/core", () => ({
			invoke: mockInvoke,
		}));

		vi.doMock("../../stores/repoDefaults", () => ({
			repoDefaultsStore: { state: mockDefaults },
		}));

		store = (await import("../../stores/repoSettings")).repoSettingsStore;
	});

	describe("get()", () => {
		it("returns undefined for unknown repo", () => {
			testInScope(() => {
				expect(store.get("/unknown")).toBeUndefined();
			});
		});
	});

	describe("getOrCreate()", () => {
		it("creates settings for new repo with null overridable fields (inheriting from global)", () => {
			testInScope(() => {
				const settings = store.getOrCreate("/repo", "my-repo");
				expect(settings.path).toBe("/repo");
				expect(settings.displayName).toBe("my-repo");
				// Overridable fields default to null (inherit from global defaults)
				expect(settings.baseBranch).toBeNull();
				expect(settings.copyIgnoredFiles).toBeNull();
				expect(settings.copyUntrackedFiles).toBeNull();
				expect(settings.setupScript).toBeNull();
				expect(settings.runScript).toBeNull();
				// Non-overridable fields remain non-nullable
				expect(settings.color).toBe("");
			});
		});

		it("returns existing settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main" });
				const settings = store.getOrCreate("/repo", "my-repo");
				expect(settings.baseBranch).toBe("main");
			});
		});

		it("persists via invoke", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				expect(mockInvoke).toHaveBeenCalledWith("save_repo_settings", {
					config: expect.objectContaining({
						repos: expect.objectContaining({
							"/repo": expect.objectContaining({ path: "/repo" }),
						}),
					}),
				});
			});
		});

		it("persists in snake_case — the wire shape Rust's RepoSettingsEntry expects, not the camelCase store shape", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				const call = lastInvokeCall("save_repo_settings");
				const entry = (call![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos["/repo"];
				// Rust's #[serde(default)] silently drops any key it doesn't recognize —
				// camelCase keys like `displayName` would vanish on save without failing
				// loudly, which is exactly what happened before this wire layer existed.
				expect(entry).toEqual(
					expect.objectContaining({
						path: "/repo",
						display_name: "my-repo",
						base_branch: null,
						copy_ignored_files: null,
						auto_fetch_interval_minutes: null,
						pr_hide_drafts: null,
						terminal_meta_hotkeys: null,
						auto_consolidate_worktrees: false,
					}),
				);
				expect(entry.displayName).toBeUndefined();
				expect(entry.baseBranch).toBeUndefined();
			});
		});
	});

	describe("update()", () => {
		it("updates existing settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
				expect(store.get("/repo")?.baseBranch).toBe("main");
				expect(store.get("/repo")?.setupScript).toBe("npm install");
			});
		});

		it("can set overridable fields back to null (inherit)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main" });
				store.update("/repo", { baseBranch: null });
				expect(store.get("/repo")?.baseBranch).toBeNull();
			});
		});

		it("ignores updates for unknown repos", () => {
			testInScope(() => {
				store.update("/unknown", { baseBranch: "main" }); // Should not throw
			});
		});

		it("persists an override in snake_case", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { prHideDrafts: true, autoFetchIntervalMinutes: 15 });
				const call = lastInvokeCall("save_repo_settings");
				const entry = (call![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos["/repo"];
				expect(entry.pr_hide_drafts).toBe(true);
				expect(entry.auto_fetch_interval_minutes).toBe(15);
			});
		});

		it("persists and rehydrates mcpUpstreams — the only RepoSettings field with security consequences", () => {
			// mcpUpstreams is a per-repo allowlist of which MCP upstream servers an agent
			// may reach; null means "no restriction". Before the wire-conversion fix this
			// field silently failed to persist like every other one, so a user's attempt
			// to restrict a sensitive repo to a subset of upstreams (via mcpPopup.ts's
			// toggleServerForProject, which mirrors the value here purely to keep this
			// store's in-memory copy in sync — the actual write goes through the dedicated
			// set_project_mcp_upstreams Rust command) would fail open back to "all
			// servers allowed" the next time this store re-saved the whole map for any
			// unrelated reason.
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { mcpUpstreams: ["github"] });
				const call = lastInvokeCall("save_repo_settings");
				const entry = (call![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos["/repo"];
				expect(entry.mcp_upstreams).toEqual(["github"]);
			});
		});
	});

	describe("getEffective()", () => {
		it("returns global defaults for a new repo with null overrides", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("automatic"); // from global default
				expect(effective!.copyIgnoredFiles).toBe(false);
				expect(effective!.copyUntrackedFiles).toBe(false);
				expect(effective!.setupScript).toBe("");
				expect(effective!.runScript).toBe("");
			});
		});

		it("uses per-repo override when set", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", copyIgnoredFiles: true });

				setMockDefaults("baseBranch", "develop"); // global default is different
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("main"); // repo override wins
				expect(effective!.copyIgnoredFiles).toBe(true);
			});
		});

		it("falls back to global default when field is null", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				// baseBranch is null (inherit) but global says "develop"
				setMockDefaults("baseBranch", "develop");
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("develop");
			});
		});

		it("returns non-nullable effective settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				// All fields must be non-null
				expect(effective!.baseBranch).not.toBeNull();
				expect(effective!.copyIgnoredFiles).not.toBeNull();
				expect(effective!.setupScript).not.toBeNull();
			});
		});

		it("returns undefined for unknown repo", () => {
			testInScope(() => {
				expect(store.getEffective("/unknown")).toBeUndefined();
			});
		});

		it("returns archiveScript from global default when not overridden", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				setMockDefaults("archiveScript", "cleanup.sh");
				const effective = store.getEffective("/repo");
				expect(effective!.archiveScript).toBe("cleanup.sh");
			});
		});

		it("uses per-repo archiveScript override when set", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { archiveScript: "my-cleanup.sh" });
				setMockDefaults("archiveScript", "global-cleanup.sh");
				const effective = store.getEffective("/repo");
				expect(effective!.archiveScript).toBe("my-cleanup.sh");
			});
		});

		// A tri-state field's persistence (toWire/fromWire) and its resolution
		// (this function) are two separately-tested halves that never meet in
		// the tests above — every getEffective() case here starts from an
		// in-memory settings object, never from a value that actually went
		// through the wire. These two round-trip through a real save→hydrate
		// cycle, the same path a TriStateToggle's "Use global"/explicit-off
		// choice takes in production.
		it("a tri-state field left at null survives save→hydrate and resolves to the global default", async () => {
			mockDefaults.copyIgnoredFiles = true;

			await testInScopeAsync(async () => {
				store.getOrCreate("/repo", "my-repo");
				// getOrCreate leaves copyIgnoredFiles at its null (inherit) default —
				// capture exactly what was persisted for it.
				const saved = lastInvokeCall("save_repo_settings");
				const wire = (saved![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos["/repo"];
				expect(wire.copy_ignored_files).toBeNull();

				// Fresh store, as if the app restarted: hydrate from exactly that wire payload.
				mockInvoke.mockResolvedValueOnce({ repos: { "/repo": wire } });
				await store.hydrate();

				expect(store.get("/repo")?.copyIgnoredFiles).toBeNull();
				expect(store.getEffective("/repo")?.copyIgnoredFiles).toBe(true);
			});
		});

		it("a tri-state field explicitly set to false survives save→hydrate and resolves to false, not the global default", async () => {
			mockDefaults.copyIgnoredFiles = true;

			await testInScopeAsync(async () => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { copyIgnoredFiles: false });
				const saved = lastInvokeCall("save_repo_settings");
				const wire = (saved![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos["/repo"];
				expect(wire.copy_ignored_files).toBe(false);

				mockInvoke.mockResolvedValueOnce({ repos: { "/repo": wire } });
				await store.hydrate();

				expect(store.get("/repo")?.copyIgnoredFiles).toBe(false);
				expect(store.getEffective("/repo")?.copyIgnoredFiles).toBe(false);
			});
		});

		it("preserves non-overridable fields (path, displayName, color)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { color: "#ff0000" });
				const effective = store.getEffective("/repo");
				expect(effective!.path).toBe("/repo");
				expect(effective!.displayName).toBe("my-repo");
				expect(effective!.color).toBe("#ff0000");
			});
		});
	});

	describe("three-tier getEffective() with local config", () => {
		it("uses .tuic.json values when per-repo setting is null", async () => {
			await testInScopeAsync(async () => {
				// Simulate Tauri returning a local config from .tuic.json
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_repo_local_config") {
						return { base_branch: "develop", setup_script: "make setup" };
					}
					return undefined;
				});

				store.getOrCreate("/repo", "my-repo");
				await store.loadLocalConfig("/repo");

				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				// .tuic.json overrides global default for non-script fields
				expect(effective!.baseBranch).toBe("develop");
				// SECURITY: script fields from .tuic.json are intentionally NOT merged
				// (a malicious repo could inject shell commands via committed .tuic.json)
				expect(effective!.setupScript).toBe(""); // from global default, NOT .tuic.json
				// Global default still applies for fields not in .tuic.json
				expect(effective!.runScript).toBe(""); // from global default
			});
		});

		it("per-repo setting overrides .tuic.json", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_repo_local_config") {
						return { base_branch: "develop", setup_script: "make setup" };
					}
					return undefined;
				});

				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main" }); // per-repo override
				await store.loadLocalConfig("/repo");

				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				// per-repo overrides .tuic.json
				expect(effective!.baseBranch).toBe("main");
				// SECURITY: script fields from .tuic.json are NOT merged — falls back to global default
				expect(effective!.setupScript).toBe(""); // from global default, NOT .tuic.json
			});
		});

		it("returns undefined for missing .tuic.json (no local config cached)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				// No loadLocalConfig called — should fall back to two-tier
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("automatic"); // global default
			});
		});

		it("handles null from Tauri (no .tuic.json file)", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_repo_local_config") return null;
					return undefined;
				});

				store.getOrCreate("/repo", "my-repo");
				await store.loadLocalConfig("/repo");

				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("automatic"); // global default
			});
		});
	});

	describe("remove()", () => {
		it("removes settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.remove("/repo");
				expect(store.get("/repo")).toBeUndefined();
			});
		});

		it("clears activeRepoPath if removed", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.setActiveRepo("/repo");
				store.remove("/repo");
				expect(store.state.activeRepoPath).toBeNull();
			});
		});
	});

	describe("hasCustomSettings()", () => {
		it("returns false for defaults", async () => {
			await testInScopeAsync(async () => {
				store.getOrCreate("/repo", "my-repo");
				mockInvoke.mockResolvedValueOnce(false);
				expect(await store.hasCustomSettings("/repo")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith("check_has_custom_settings", { path: "/repo" });
			});
		});

		it("returns false for unknown repos", async () => {
			await testInScopeAsync(async () => {
				expect(await store.hasCustomSettings("/unknown")).toBe(false);
			});
		});
	});

	describe("reset()", () => {
		it("resets overridable fields to null (inherit from global)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
				store.reset("/repo");
				expect(store.get("/repo")?.baseBranch).toBeNull();
				expect(store.get("/repo")?.setupScript).toBeNull();
				expect(store.get("/repo")?.copyIgnoredFiles).toBeNull();
			});
		});

		it("preserves non-overridable fields (displayName, color)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { color: "#ff0000" });
				store.reset("/repo");
				expect(store.get("/repo")?.displayName).toBe("my-repo");
				expect(store.get("/repo")?.color).toBe("#ff0000");
			});
		});

		it("persists the reset (all overridable fields null) in snake_case", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", prHideDrafts: true });
				store.reset("/repo");
				const call = lastInvokeCall("save_repo_settings");
				const entry = (call![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos["/repo"];
				expect(entry.base_branch).toBeNull();
				expect(entry.pr_hide_drafts).toBeNull();
				expect(entry.display_name).toBe("my-repo");
			});
		});
	});

	describe("getAll()", () => {
		it("returns all settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo1", "repo1");
				store.getOrCreate("/repo2", "repo2");
				expect(store.getAll()).toHaveLength(2);
			});
		});
	});

	describe("setActiveRepo()", () => {
		it("sets active repo path", () => {
			testInScope(() => {
				store.setActiveRepo("/repo");
				expect(store.state.activeRepoPath).toBe("/repo");
			});
		});
	});

	describe("hydrate()", () => {
		it("loads settings from Rust backend — the real snake_case RepoSettingsEntry shape", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						display_name: "my-repo",
						base_branch: "main",
						copy_ignored_files: null,
						copy_untracked_files: null,
						setup_script: null,
						run_script: null,
						pr_hide_drafts: true,
						terminal_meta_hotkeys: false,
						auto_consolidate_worktrees: true,
					},
				},
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				const settings = store.get("/repo");
				expect(settings?.baseBranch).toBe("main");
				expect(settings?.displayName).toBe("my-repo");
				expect(settings?.prHideDrafts).toBe(true);
				expect(settings?.terminalMetaHotkeys).toBe(false);
				expect(settings?.autoConsolidateWorktrees).toBe(true);
				expect(mockInvoke).toHaveBeenCalledWith("load_repo_settings");
			});
		});

		it("rehydrates mcpUpstreams from the wire shape", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": { path: "/repo", display_name: "my-repo", mcp_upstreams: ["github"] },
				},
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.get("/repo")?.mcpUpstreams).toEqual(["github"]);
			});
		});

		it("fills in defaults for fields absent from an older on-disk entry", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": { path: "/repo", display_name: "my-repo" },
				},
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				const settings = store.get("/repo");
				expect(settings?.baseBranch).toBeNull();
				expect(settings?.color).toBe("");
				expect(settings?.autoConsolidateWorktrees).toBe(false);
				expect(settings?.branchLabels).toEqual({});
			});
		});

		it("migrates from localStorage on first run, converting the legacy camelCase entries to the wire shape", async () => {
			localStorage.setItem(
				"tui-commander-repo-settings",
				JSON.stringify({
					"/repo": { path: "/repo", displayName: "my-repo", baseBranch: "main" },
				}),
			);
			mockInvoke.mockResolvedValueOnce(undefined); // save_repo_settings migration
			mockInvoke.mockResolvedValueOnce({ repos: {} }); // load_repo_settings

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-repo-settings")).toBeNull();
				const migrationCall = lastInvokeCall("save_repo_settings");
				const migratedEntry = (migrationCall![1] as { config: { repos: Record<string, WireRepoEntry> } }).config.repos[
					"/repo"
				];
				expect(migratedEntry.display_name).toBe("my-repo");
				expect(migratedEntry.base_branch).toBe("main");
				expect(migratedEntry.displayName).toBeUndefined();
			});
		});
	});
	// ---- Per-field access (F73) ----
	//
	// getEffective() reads ~50 signals across four stores and allocates a
	// 24-field object. Call sites that want one field (a branch label, a
	// terminalMetaHotkeys flag) must not wake on every one of those.

	describe("per-field effective access", () => {
		let dispose: (() => void) | undefined;

		afterEach(() => {
			dispose?.();
			dispose = undefined;
		});

		const flush = () => new Promise<void>((resolve) => queueMicrotask(resolve));

		it("reading one field does not subscribe to unrelated global defaults", async () => {
			store.getOrCreate("/repo", "my-repo");
			let runs = 0;

			createRoot((d) => {
				dispose = d;
				createEffect(() => {
					// terminalMetaHotkeys resolves from the repo field alone.
					void store.getEffectiveField("/repo", "terminalMetaHotkeys");
					runs++;
				});
			});
			await flush();
			runs = 0;

			// A default this field never consults.
			setMockDefaults("baseBranch", "develop");
			await flush();

			expect(runs).toBe(0);
		});

		// The contrast that motivates getEffectiveField: the same one-field read
		// through getEffective wakes on a default the field never consults,
		// because getEffective touches all 53 properties on every call.
		it("getEffective wakes a one-field reader on an unrelated default", async () => {
			store.getOrCreate("/repo", "my-repo");
			let runs = 0;

			createRoot((d) => {
				dispose = d;
				createEffect(() => {
					void store.getEffective("/repo")?.terminalMetaHotkeys;
					runs++;
				});
			});
			await flush();
			runs = 0;

			setMockDefaults("baseBranch", "develop");
			await flush();

			expect(runs).toBe(1);
		});

		it("still wakes when the field's own inheritance chain changes", async () => {
			store.getOrCreate("/repo", "my-repo");
			let seen: string | undefined;
			let runs = 0;

			createRoot((d) => {
				dispose = d;
				createEffect(() => {
					seen = store.getEffectiveField("/repo", "baseBranch");
					runs++;
				});
			});
			await flush();
			expect(seen).toBe("automatic");
			runs = 0;

			setMockDefaults("baseBranch", "develop");
			await flush();

			expect(runs).toBe(1);
			expect(seen).toBe("develop");
		});

		it("agrees with getEffective on every field", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", color: "#fff", autoFetchIntervalMinutes: 7 });
				setMockDefaults("archiveScript", "cleanup.sh");

				const effective = store.getEffective("/repo")!;
				for (const key of Object.keys(effective) as (keyof typeof effective)[]) {
					expect(store.getEffectiveField("/repo", key)).toEqual(effective[key]);
				}
			});
		});

		it("returns undefined for an unknown repo", () => {
			testInScope(() => {
				expect(store.getEffectiveField("/unknown", "baseBranch")).toBeUndefined();
			});
		});
	});
});
