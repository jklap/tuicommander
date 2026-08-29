import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("settingsStore", () => {
	let store: typeof import("../../stores/settings").settingsStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		localStorage.clear();
		mockInvoke.mockReset().mockResolvedValue(undefined);

		vi.doMock("@tauri-apps/api/core", () => ({
			invoke: mockInvoke,
		}));

		store = (await import("../../stores/settings")).settingsStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	/** Hydrate the store so save() is unlocked — pre-hydrate saves are refused
	 *  to avoid clobbering config.json with defaults. */
	async function hydrateStore(): Promise<void> {
		mockInvoke.mockResolvedValueOnce({
			shell: null,
			font_family: "JetBrains Mono",
			font_size: 14,
			theme: "vscode-dark",
			mcp_server_enabled: false,
			ide: "vscode",
			default_font_size: 13,
		});
		await store.hydrate();
	}

	function saveConfigCalls(): unknown[][] {
		return mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_config");
	}

	describe("pre-hydrate write protection", () => {
		it("does not persist before hydrate", async () => {
			await testInScopeAsync(async () => {
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(0);
			});
		});

		it("does not persist when hydrate failed", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("no backend"));
			await testInScopeAsync(async () => {
				await store.hydrate();
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(0);
			});
		});

		it("persists after successful hydrate", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(1);
			});
		});
	});

	describe("load-modify-save preserves foreign-owned fields", () => {
		// Regression: a general-settings save must NOT clobber fields owned by
		// other surfaces (services.server.enabled → ServicesTab, global_hotkey →
		// set_global_hotkey command). The old buildConfig() rebuilt the whole
		// config from a stale hydrate snapshot, wiping the web-server toggle and
		// global hotkey on the next restart.
		it("keeps services.server.enabled and global_hotkey set by other writers", async () => {
			await testInScopeAsync(async () => {
				// Hydrate with a STALE snapshot: server OFF, no hotkey.
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					mcp_server_enabled: false,
					ide: "vscode",
					default_font_size: 13,
					global_hotkey: null,
					services: { server: { enabled: false, port: 9876 } },
				});
				await store.hydrate();

				// Another surface has since enabled the server + set a hotkey; the
				// fresh load_config the save path performs reflects that on disk.
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					mcp_server_enabled: true,
					ide: "vscode",
					default_font_size: 13,
					global_hotkey: "CommandOrControl+1",
					services: { server: { enabled: true, port: 9876 } },
				});

				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();

				const calls = saveConfigCalls();
				expect(calls).toHaveLength(1);
				const saved = (
					calls[0][1] as {
						config: {
							ide: string;
							services: { server: { enabled: boolean } };
							global_hotkey: string | null;
							mcp_server_enabled: boolean;
						};
					}
				).config;
				expect(saved.ide).toBe("cursor"); // owned field applied
				expect(saved.services.server.enabled).toBe(true); // preserved
				expect(saved.global_hotkey).toBe("CommandOrControl+1"); // preserved
				expect(saved.mcp_server_enabled).toBe(true); // preserved
			});
		});

		it("skips the save (no clobber) when the fresh load_config fails", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				mockInvoke.mockRejectedValueOnce(new Error("backend down"));
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(0);
			});
		});
	});

	describe("defaults", () => {
		it("has correct default values", () => {
			testInScope(() => {
				expect(store.state.ide).toBe("vscode");
				expect(store.state.font).toBe("JetBrains Mono");
				expect(store.state.defaultFontSize).toBe(13);
				expect(store.state.confirmBeforeQuit).toBe(true);
				expect(store.state.confirmBeforeClosingTab).toBe(true);
				expect(store.state.splitTabMode).toBe("separate");
				expect(store.state.restoreWindowGeometry).toBe(true);
				expect(store.state.restoreShellTerminals).toBe(true);
				expect(store.state.restoreScrollback).toBe(false);
				expect(store.state.restoreScrollbackLines).toBe(1000);
				expect(store.state.tabOrderingMode).toBe("grouped-by-type");
				expect(store.state.tabCyclingAllTypes).toBe(false);
				expect(store.state.tabTreeEnabled).toBe(false);
				expect(store.state.maxTabNameLength).toBe(25);
			});
		});
	});

	describe("setIde()", () => {
		it("updates IDE preference in state", () => {
			testInScope(() => {
				store.setIde("cursor");
				expect(store.state.ide).toBe("cursor");
			});
		});

		it("persists IDE via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIde("cursor");
				// Not yet saved (debounced)
				expect(mockInvoke).not.toHaveBeenCalledWith("save_config", expect.anything());
				// Advance past debounce
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ ide: "cursor" }),
				});
			});
		});

		it("coalesces rapid changes into single save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIde("cursor");
				store.setIde("zed");
				store.setIde("windsurf");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				// Only one save_config call with the final value
				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_config");
				expect(saveCalls).toHaveLength(1);
				expect(saveCalls[0][1].config.ide).toBe("windsurf");
			});
		});
	});

	describe("setFont()", () => {
		it("updates font in store state", () => {
			testInScope(() => {
				store.setFont("Fira Code");
				expect(store.state.font).toBe("Fira Code");
			});
		});

		it("persists font via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setFont("Fira Code");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ font_family: "Fira Code" }),
				});
			});
		});
	});

	describe("block display settings", () => {
		it("setShowBlockTimestamps persists show_block_timestamps", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShowBlockTimestamps(false);
				expect(store.state.showBlockTimestamps).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ show_block_timestamps: false }),
				});
			});
		});

		it("setShowBlockMarks persists show_block_marks", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShowBlockMarks(false);
				expect(store.state.showBlockMarks).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ show_block_marks: false }),
				});
			});
		});

		it("setShowPromptMarks persists show_prompt_marks", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShowPromptMarks(false);
				expect(store.state.showPromptMarks).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ show_prompt_marks: false }),
				});
			});
		});

		it("setBlockFoldingEnabled persists block_folding_enabled", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setBlockFoldingEnabled(false);
				expect(store.state.blockFoldingEnabled).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ block_folding_enabled: false }),
				});
			});
		});

		it("defaults all four to true when absent from the hydrated config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.state.showBlockTimestamps).toBe(true);
				expect(store.state.showBlockMarks).toBe(true);
				expect(store.state.showPromptMarks).toBe(true);
				expect(store.state.blockFoldingEnabled).toBe(true);
			});
		});
	});

	describe("setLinkActivation()", () => {
		it("persists terminal_link_activation", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setLinkActivation("modifier");
				expect(store.state.linkActivation).toBe("modifier");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ terminal_link_activation: "modifier" }),
				});
			});
		});

		it("defaults to click when absent from the hydrated config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.state.linkActivation).toBe("click");
			});
		});

		it("falls back to click for an invalid persisted value", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					mcp_server_enabled: false,
					ide: "vscode",
					default_font_size: 13,
					terminal_link_activation: "bogus",
				});
				await store.hydrate();
				expect(store.state.linkActivation).toBe("click");
			});
		});
	});

	describe("smart selection settings", () => {
		it("defaults double-click action to smart and word mode to characters", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.state.doubleClickAction).toBe("smart");
				expect(store.state.wordSelectionMode).toBe("characters");
				expect(store.state.wordSelectionRegex).toBe("");
				expect(store.state.smartSelectionRules).toEqual([]);
				// Matches WORD_SEPARATOR_RE's punctuation class exactly.
				expect(store.state.wordSeparators).toBe(" \"'`(){}[]<>|;:,.!?@#$%^&*~=+/\\");
			});
		});

		it("persists setDoubleClickAction and falls back to smart for an invalid persisted value", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setDoubleClickAction("word");
				expect(store.state.doubleClickAction).toBe("word");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ double_click_action: "word" }),
				});

				mockInvoke.mockResolvedValueOnce({ double_click_action: "bogus" });
				await store.hydrate();
				expect(store.state.doubleClickAction).toBe("smart");
			});
		});

		it("persists setWordSelectionMode and falls back to characters for an invalid persisted value", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setWordSelectionMode("regex");
				expect(store.state.wordSelectionMode).toBe("regex");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ word_selection_mode: "regex" }),
				});

				mockInvoke.mockResolvedValueOnce({ word_selection_mode: "bogus" });
				await store.hydrate();
				expect(store.state.wordSelectionMode).toBe("characters");
			});
		});

		it("persists setWordSeparators and setWordSelectionRegex", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setWordSeparators("-_");
				store.setWordSelectionRegex("https://|[a-z]+");
				expect(store.state.wordSeparators).toBe("-_");
				expect(store.state.wordSelectionRegex).toBe("https://|[a-z]+");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ word_separators: "-_", word_selection_regex: "https://|[a-z]+" }),
				});
			});
		});

		it("round-trips smart-selection rules (including nested actions) through hydrate and save", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					smart_selection_rules: [
						{
							id: "r1",
							name: "Git SHA",
							regex: "[0-9a-f]{7,40}",
							precision: "high",
							enabled: true,
							actions: [{ kind: "run_command", title: "Show commit", parameter: "git show \\0", is_default: true }],
						},
					],
				});
				await store.hydrate();
				expect(store.state.smartSelectionRules).toEqual([
					{
						id: "r1",
						name: "Git SHA",
						regex: "[0-9a-f]{7,40}",
						precision: "high",
						enabled: true,
						actions: [{ kind: "run_command", title: "Show commit", parameter: "git show \\0", isDefault: true }],
					},
				]);

				store.setSmartSelectionRules([
					{
						...store.state.smartSelectionRules[0],
						actions: [{ ...store.state.smartSelectionRules[0].actions[0], isDefault: false }],
					},
				]);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({
						smart_selection_rules: [
							expect.objectContaining({
								actions: [expect.objectContaining({ is_default: false })],
							}),
						],
					}),
				});
			});
		});
	});

	describe("getFontFamily()", () => {
		it("returns CSS font family string", () => {
			testInScope(() => {
				const family = store.getFontFamily();
				expect(family).toContain("JetBrains");
				expect(family).toContain("monospace");
			});
		});
	});

	describe("getIdeName()", () => {
		it("returns display name for IDE", () => {
			testInScope(() => {
				expect(store.getIdeName()).toBe("VS Code");
				store.setIde("zed");
				expect(store.getIdeName()).toBe("Zed");
			});
		});
	});

	describe("loadFontFromConfig()", () => {
		it("applies font from hydrated config cache", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Hack",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 12,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				// Change font locally
				store.setFont("Fira Code");
				expect(store.state.font).toBe("Fira Code");
				// Re-apply from cache (no IPC)
				store.loadFontFromConfig();
				expect(store.state.font).toBe("Hack");
				// No extra load_config call — uses hydrate cache
				const loadCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "load_config");
				expect(loadCalls).toHaveLength(1); // only from hydrate
			});
		});

		it("no-op before hydrate", () => {
			testInScope(() => {
				store.loadFontFromConfig();
				expect(store.state.font).toBe("JetBrains Mono");
			});
		});
	});

	describe("hydrate()", () => {
		it("loads settings from Rust config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Hack",
				font_size: 14,
				theme: "tokyo-night",

				mcp_server_enabled: false,
				ide: "zed",
				default_font_size: 16,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.font).toBe("Hack");
				expect(store.state.ide).toBe("zed");
				expect(store.state.defaultFontSize).toBe(16);
			});
		});

		it("loads window-geometry / session-restore fields from Rust config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "vscode-dark",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 13,
				restore_window_geometry: false,
				restore_shell_terminals: false,
				restore_scrollback: true,
				restore_scrollback_lines: 2500,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.restoreWindowGeometry).toBe(false);
				expect(store.state.restoreShellTerminals).toBe(false);
				expect(store.state.restoreScrollback).toBe(true);
				expect(store.state.restoreScrollbackLines).toBe(2500);
			});
		});

		it("respects an explicit persisted 0 for restoreScrollbackLines (not the 1000 default)", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "vscode-dark",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 13,
				restore_scrollback_lines: 0,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.restoreScrollbackLines).toBe(0);
			});
		});

		it("falls back to defaults when window-geometry / session-restore fields are absent from config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.state.restoreWindowGeometry).toBe(true);
				expect(store.state.restoreShellTerminals).toBe(true);
				expect(store.state.restoreScrollback).toBe(false);
				expect(store.state.restoreScrollbackLines).toBe(1000);
			});
		});

		it("migrates legacy IDE from localStorage", async () => {
			localStorage.setItem("tui-commander-default-ide", "cursor");
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 12,
			}); // load_config for migration
			mockInvoke.mockResolvedValueOnce(undefined); // save_config for migration
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "cursor",
				default_font_size: 12,
			}); // load_config after migration

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-default-ide")).toBeNull();
			});
		});

		it("falls back to defaults for invalid values from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Comic Sans",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "invalid-ide",
				default_font_size: 12,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.font).toBe("JetBrains Mono");
				expect(store.state.ide).toBe("vscode");
			});
		});

		it("keeps defaults on invoke failure", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("no backend"));

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.font).toBe("JetBrains Mono");
				expect(store.state.ide).toBe("vscode");
			});
		});
	});

	describe("setDefaultFontSize()", () => {
		it("clamps font size to valid range", () => {
			testInScope(() => {
				store.setDefaultFontSize(5);
				expect(store.state.defaultFontSize).toBe(8);
				store.setDefaultFontSize(50);
				expect(store.state.defaultFontSize).toBe(32);
				store.setDefaultFontSize(16);
				expect(store.state.defaultFontSize).toBe(16);
			});
		});
	});

	describe("setShell()", () => {
		it("sets custom shell", () => {
			testInScope(() => {
				store.setShell("/bin/zsh");
				expect(store.state.shell).toBe("/bin/zsh");
			});
		});

		it("trims whitespace and sets null for empty string", () => {
			testInScope(() => {
				store.setShell("  ");
				expect(store.state.shell).toBeNull();
			});
		});

		it("persists shell via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShell("/bin/zsh");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ shell: "/bin/zsh" }),
				});
			});
		});
	});

	describe("setTheme()", () => {
		it("sets theme and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setTheme("dracula");
				expect(store.state.theme).toBe("dracula");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ theme: "dracula" }),
				});
			});
		});
	});

	describe("setSplitTabMode()", () => {
		it("sets split tab mode and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setSplitTabMode("unified");
				expect(store.state.splitTabMode).toBe("unified");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ split_tab_mode: "unified" }),
				});
			});
		});
	});

	describe("setTabOrderingMode()", () => {
		it("sets tab ordering mode and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setTabOrderingMode("terminals-first");
				expect(store.state.tabOrderingMode).toBe("terminals-first");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ tab_ordering_mode: "terminals-first" }),
				});
			});
		});
	});

	describe("setTabTreeEnabled()", () => {
		it("sets tab tree enabled and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setTabTreeEnabled(true);
				expect(store.state.tabTreeEnabled).toBe(true);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ tab_tree_enabled: true }),
				});
			});
		});
	});

	describe("setTabCyclingAllTypes()", () => {
		it("sets tab cycling all types and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setTabCyclingAllTypes(true);
				expect(store.state.tabCyclingAllTypes).toBe(true);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ tab_cycling_all_types: true }),
				});
			});
		});
	});

	describe("setMaxTabNameLength()", () => {
		it("sets max tab name length and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setMaxTabNameLength(45);
				expect(store.state.maxTabNameLength).toBe(45);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ max_tab_name_length: 45 }),
				});
			});
		});

		it("clamps to the 10-60 range", () => {
			testInScope(() => {
				store.setMaxTabNameLength(5);
				expect(store.state.maxTabNameLength).toBe(10);
				store.setMaxTabNameLength(100);
				expect(store.state.maxTabNameLength).toBe(60);
			});
		});
	});

	describe("setIndicatorColor()", () => {
		it("adds a new override and persists it via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#ff00ff" }]);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({
						indicator_overrides: [{ id: "terminal.busy", color: "#ff00ff" }],
					}),
				});
			});
		});

		it("replaces an existing override's color in place rather than duplicating it", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorColor("terminal.busy", "#00ff00");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#00ff00" }]);
			});
		});

		it("rejects an unsafe color and leaves state untouched", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "javascript:alert(1)");
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});

		it("accepts var(--...) references so a theme keeps flowing through", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "var(--accent)");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "var(--accent)" }]);
			});
		});

		it("tracks independent overrides for different indicator ids", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorColor("pr.conflict", "#00ff00");
				expect(store.state.indicatorOverrides).toEqual([
					{ id: "terminal.busy", color: "#ff00ff" },
					{ id: "pr.conflict", color: "#00ff00" },
				]);
			});
		});
	});

	describe("setIndicatorIcon()", () => {
		it("adds a new override with the icon field and persists it", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIndicatorIcon("terminal.busy", "ring");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "ring" }]);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ indicator_overrides: [{ id: "terminal.busy", icon: "ring" }] }),
				});
			});
		});

		it("adds an icon override alongside an existing color override on the same id", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorIcon("terminal.busy", "square");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#ff00ff", icon: "square" }]);
			});
		});

		it("replaces an existing icon override in place rather than duplicating it", () => {
			testInScope(() => {
				store.setIndicatorIcon("terminal.busy", "ring");
				store.setIndicatorIcon("terminal.busy", "square");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "square" }]);
			});
		});

		it("rejects an unknown icon id and leaves state untouched", () => {
			testInScope(() => {
				store.setIndicatorIcon("terminal.busy", "not-a-real-icon");
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});
	});

	describe("setIndicatorAnimation()", () => {
		it("adds a new override with the animation field and persists it", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIndicatorAnimation("terminal.busy", "blink");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", animation: "blink" }]);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ indicator_overrides: [{ id: "terminal.busy", animation: "blink" }] }),
				});
			});
		});

		it("replaces an existing animation override in place rather than duplicating it", () => {
			testInScope(() => {
				store.setIndicatorAnimation("terminal.busy", "blink");
				store.setIndicatorAnimation("terminal.busy", "breathe");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", animation: "breathe" }]);
			});
		});

		it("rejects an unknown animation id and leaves state untouched", () => {
			testInScope(() => {
				store.setIndicatorAnimation("terminal.busy", "not-a-real-animation");
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});
	});

	describe("clearIndicatorOverride()", () => {
		it("removes exactly the named override, leaving others untouched", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorColor("pr.conflict", "#00ff00");
				store.clearIndicatorOverride("terminal.busy");
				expect(store.state.indicatorOverrides).toEqual([{ id: "pr.conflict", color: "#00ff00" }]);
			});
		});

		it("persists the removal via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.clearIndicatorOverride("terminal.busy");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenLastCalledWith("save_config", {
					config: expect.objectContaining({ indicator_overrides: [] }),
				});
			});
		});

		it("is a no-op when the id has no override", () => {
			testInScope(() => {
				store.clearIndicatorOverride("terminal.busy");
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});
	});

	describe("clearIndicatorField()", () => {
		it("clears one field and leaves sibling fields on the same override standing", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorIcon("terminal.busy", "diamond");
				store.clearIndicatorField("terminal.busy", "color");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "diamond" }]);
			});
		});

		it("drops the override row entirely once its last field is cleared", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.clearIndicatorField("terminal.busy", "color");
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});

		it("leaves other indicators' overrides untouched", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorColor("pr.conflict", "#00ff00");
				store.clearIndicatorField("terminal.busy", "color");
				expect(store.state.indicatorOverrides).toEqual([{ id: "pr.conflict", color: "#00ff00" }]);
			});
		});

		it("persists the field removal via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.clearIndicatorField("terminal.busy", "color");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenLastCalledWith("save_config", {
					config: expect.objectContaining({ indicator_overrides: [] }),
				});
			});
		});

		it("is a no-op for an id with no override", () => {
			testInScope(() => {
				store.clearIndicatorField("terminal.busy", "color");
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});

		it("is a no-op when clearing a field that was never set on an existing override", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.clearIndicatorField("terminal.busy", "animation");
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#ff00ff" }]);
			});
		});
	});

	describe("resetAllIndicators()", () => {
		it("clears every override at once", () => {
			testInScope(() => {
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.setIndicatorColor("pr.conflict", "#00ff00");
				store.resetAllIndicators();
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});

		it("persists the reset via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIndicatorColor("terminal.busy", "#ff00ff");
				store.resetAllIndicators();
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenLastCalledWith("save_config", {
					config: expect.objectContaining({ indicator_overrides: [] }),
				});
			});
		});
	});

	describe("visibility toggles default to true", () => {
		it("has correct defaults", () => {
			testInScope(() => {
				expect(store.state.showDiffStats).toBe(true);
				expect(store.state.showPrBadges).toBe(true);
				expect(store.state.showGitState).toBe(true);
				expect(store.state.tabTypeHighlighting).toBe(true);
			});
		});
	});

	describe("setShowDiffStats()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShowDiffStats(false);
				expect(store.state.showDiffStats).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ show_diff_stats: false }),
				});
			});
		});
	});

	describe("setShowPrBadges()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShowPrBadges(false);
				expect(store.state.showPrBadges).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ show_pr_badges: false }),
				});
			});
		});
	});

	describe("setShowGitState()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShowGitState(false);
				expect(store.state.showGitState).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ show_git_state: false }),
				});
			});
		});
	});

	describe("setTabTypeHighlighting()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setTabTypeHighlighting(false);
				expect(store.state.tabTypeHighlighting).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ tab_type_highlighting: false }),
				});
			});
		});
	});

	describe("hydrate() sanitizes indicator overrides from disk", () => {
		it("drops an override for an id the registry doesn't know about", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					ide: "vscode",
					default_font_size: 13,
					indicator_overrides: [{ id: "not.a.real.indicator", color: "#ff00ff" }],
				});
				await store.hydrate();
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});

		it("drops an unsafe color from a hand-edited config.json", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					ide: "vscode",
					default_font_size: 13,
					indicator_overrides: [{ id: "terminal.busy", color: "javascript:alert(1)" }],
				});
				await store.hydrate();
				expect(store.state.indicatorOverrides).toEqual([]);
			});
		});

		it("drops an unknown icon id, keeping the rest of the same override", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					ide: "vscode",
					default_font_size: 13,
					indicator_overrides: [{ id: "terminal.busy", color: "#ff00ff", icon: "not-a-real-icon" }],
				});
				await store.hydrate();
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#ff00ff" }]);
			});
		});

		it("drops an unknown animation id, keeping the rest of the same override", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					ide: "vscode",
					default_font_size: 13,
					indicator_overrides: [{ id: "terminal.busy", color: "#ff00ff", animation: "not-a-real-animation" }],
				});
				await store.hydrate();
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#ff00ff" }]);
			});
		});

		it("keeps a valid override from disk", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					ide: "vscode",
					default_font_size: 13,
					indicator_overrides: [{ id: "terminal.busy", color: "#ff00ff" }],
				});
				await store.hydrate();
				expect(store.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#ff00ff" }]);
			});
		});
	});

	describe("setConfirmBeforeQuit()", () => {
		it("updates state", () => {
			testInScope(() => {
				store.setConfirmBeforeQuit(false);
				expect(store.state.confirmBeforeQuit).toBe(false);
			});
		});
	});

	describe("setConfirmBeforeClosingTab()", () => {
		it("updates state", () => {
			testInScope(() => {
				store.setConfirmBeforeClosingTab(false);
				expect(store.state.confirmBeforeClosingTab).toBe(false);
			});
		});
	});

	describe("setRestoreWindowGeometry()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setRestoreWindowGeometry(false);
				expect(store.state.restoreWindowGeometry).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ restore_window_geometry: false }),
				});
			});
		});
	});

	describe("setRestoreShellTerminals()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setRestoreShellTerminals(false);
				expect(store.state.restoreShellTerminals).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ restore_shell_terminals: false }),
				});
			});
		});
	});

	describe("setRestoreScrollback()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setRestoreScrollback(true);
				expect(store.state.restoreScrollback).toBe(true);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ restore_scrollback: true }),
				});
			});
		});
	});

	describe("setRestoreScrollbackLines()", () => {
		it("updates state and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setRestoreScrollbackLines(2000);
				expect(store.state.restoreScrollbackLines).toBe(2000);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ restore_scrollback_lines: 2000 }),
				});
			});
		});

		it("clamps to a non-negative rounded integer", () => {
			testInScope(() => {
				store.setRestoreScrollbackLines(-5.7);
				expect(store.state.restoreScrollbackLines).toBe(0);
				store.setRestoreScrollbackLines(123.6);
				expect(store.state.restoreScrollbackLines).toBe(124);
			});
		});
	});

	describe("autoShowPrPopover", () => {
		it("defaults to true", () => {
			testInScope(() => {
				expect(store.state.autoShowPrPopover).toBe(true);
			});
		});

		it("sets autoShowPrPopover and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setAutoShowPrPopover(false);
				expect(store.state.autoShowPrPopover).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ auto_show_pr_popover: false }),
				});
			});
		});

		it("hydrates autoShowPrPopover from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 12,
				auto_show_pr_popover: false,
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.autoShowPrPopover).toBe(false);
			});
		});
	});

	describe("setIssueFilter()", () => {
		it("updates issueFilter in state", () => {
			testInScope(() => {
				store.setIssueFilter("created");
				expect(store.state.issueFilter).toBe("created");
			});
		});

		it("persists issueFilter via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIssueFilter("mentioned");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ issue_filter: "mentioned" }),
				});
			});
		});

		it("defaults to 'assigned' on hydrate with missing issue_filter", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.issueFilter).toBe("assigned");
			});
		});

		it("defaults to 'assigned' on hydrate with invalid issue_filter", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				issue_filter: "bogus_value",
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.issueFilter).toBe("assigned");
			});
		});

		it("preserves valid issue_filter on hydrate", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				issue_filter: "all",
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.issueFilter).toBe("all");
			});
		});
	});

	describe("PR visibility filters", () => {
		it("defaults prHideDrafts, prHideConflicting, prHideCiFailing to false", () => {
			testInScope(() => {
				expect(store.state.prHideDrafts).toBe(false);
				expect(store.state.prHideConflicting).toBe(false);
				expect(store.state.prHideCiFailing).toBe(false);
			});
		});

		it("setPrHideDrafts updates state", () => {
			testInScope(() => {
				store.setPrHideDrafts(true);
				expect(store.state.prHideDrafts).toBe(true);
				store.setPrHideDrafts(false);
				expect(store.state.prHideDrafts).toBe(false);
			});
		});

		it("setPrHideDrafts persists via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setPrHideDrafts(true);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ pr_hide_drafts: true }),
				});
			});
		});

		it("setPrHideConflicting updates state", () => {
			testInScope(() => {
				store.setPrHideConflicting(true);
				expect(store.state.prHideConflicting).toBe(true);
			});
		});

		it("setPrHideCiFailing updates state", () => {
			testInScope(() => {
				store.setPrHideCiFailing(true);
				expect(store.state.prHideCiFailing).toBe(true);
			});
		});

		it("hydrate restores pr filter flags from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				pr_hide_drafts: true,
				pr_hide_conflicting: true,
				pr_hide_ci_failing: false,
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.prHideDrafts).toBe(true);
				expect(store.state.prHideConflicting).toBe(true);
				expect(store.state.prHideCiFailing).toBe(false);
			});
		});
	});

	describe("custom launchers (#71)", () => {
		const launcher = {
			id: "abc",
			name: "My Editor",
			executable: "code",
			args: ["--goto", "{file}:{line}:{column}"],
			enabled: true,
		};

		it("defaults to an empty list", () => {
			testInScope(() => {
				expect(store.state.customLaunchers).toEqual([]);
			});
		});

		it("stores launchers in state and persists them via save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setCustomLaunchers([launcher]);
				expect(store.state.customLaunchers).toEqual([launcher]);

				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					config: expect.objectContaining({ custom_launchers: [launcher] }),
				});
			});
		});

		it("hydrates custom_launchers from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				custom_launchers: [launcher],
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.customLaunchers).toEqual([launcher]);
			});
		});
	});
});
