import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentRunConfig, AgentsConfig } from "../../agents";

const { mockInvoke } = vi.hoisted(() => {
	const mockInvoke = vi.fn().mockResolvedValue(undefined);
	return { mockInvoke };
});

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

// Import the store once (it's a singleton)
import { createAgentConfigsStore, agentConfigsStore as store } from "../../stores/agentConfigs";
import { testInScopeAsync } from "../helpers/store";

const configWithClaude = (): AgentsConfig => ({
	agents: {
		claude: {
			run_configs: [
				{ name: "Default", command: "claude", args: [], env: {}, is_default: true },
				{ name: "Print", command: "claude", args: ["--print"], env: {}, is_default: false },
			],
		},
	},
});

/** Helper: hydrate store with a specific config */
async function hydrateWith(config: AgentsConfig): Promise<void> {
	mockInvoke.mockResolvedValueOnce(config);
	await store.hydrate();
}

describe("agentConfigsStore", () => {
	beforeEach(() => {
		mockInvoke.mockReset().mockResolvedValue(undefined);
	});

	describe("hydrate()", () => {
		it("reports a failed load and permits a later successful retry", async () => {
			const load = vi
				.fn()
				.mockRejectedValueOnce(new Error("RPC load_agents_config failed: 404"))
				.mockResolvedValueOnce(configWithClaude());
			const machine = createAgentConfigsStore({ load, save: vi.fn() });

			await expect(machine.hydrate()).rejects.toThrow("404");
			expect(machine.state.loaded).toBe(false);
			expect(machine.state.loadError).toContain("404");
			await machine.hydrate();
			expect(machine.state.loaded).toBe(true);
			expect(machine.state.loadError).toBeNull();
			expect(machine.getDefaultConfig("claude")?.command).toBe("claude");
		});

		it("keeps the last good config when a refresh fails", async () => {
			const load = vi
				.fn()
				.mockResolvedValueOnce(configWithClaude())
				.mockRejectedValueOnce(new Error("connection lost"));
			const machine = createAgentConfigsStore({ load, save: vi.fn() });

			await machine.hydrate();
			await expect(machine.hydrate()).rejects.toThrow("connection lost");
			expect(machine.state.loaded).toBe(false);
			expect(machine.state.loadError).toContain("connection lost");
			expect(machine.getRunConfigs("claude")).toHaveLength(2);
		});

		it("does not overwrite remote config after a failed load", async () => {
			const save = vi.fn();
			const machine = createAgentConfigsStore({
				load: async () => {
					throw new Error("RPC load_agents_config failed: 404");
				},
				save,
			});
			await expect(machine.hydrate()).rejects.toThrow("404");
			await machine.addRunConfig("claude", {
				name: "new",
				command: "claude",
				args: [],
				env: {},
				is_default: true,
			});
			expect(save).not.toHaveBeenCalled();
		});

		it("accepts an empty successful response as loaded without an error", async () => {
			const machine = createAgentConfigsStore({ load: async () => ({ agents: {} }), save: vi.fn() });
			await machine.hydrate();
			expect(machine.state.loaded).toBe(true);
			expect(machine.state.loadError).toBeNull();
			expect(machine.getRunConfigs("claude")).toEqual([]);
		});

		it("loads agent configs from Rust backend", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.state.loaded).toBe(true);
				expect(store.getRunConfigs("claude")).toHaveLength(2);
				expect(store.getRunConfigs("claude")[0].name).toBe("Default");
			});
		});

		it("handles empty config gracefully", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				expect(store.state.loaded).toBe(true);
				expect(store.getRunConfigs("claude")).toHaveLength(0);
			});
		});

		it("reports a local hydrate failure without marking it loaded", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("load failed"));
			const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await expect(store.hydrate()).rejects.toThrow("load failed");
				expect(store.state.loaded).toBe(false);
				expect(store.state.loadError).toContain("load failed");
				expect(errSpy).toHaveBeenCalled();
				errSpy.mockRestore();
			});
		});
	});

	describe("getDefaultConfig()", () => {
		it("returns the config marked as default", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				const def = store.getDefaultConfig("claude");
				expect(def?.name).toBe("Default");
				expect(def?.is_default).toBe(true);
			});
		});

		it("returns first config if none marked as default", async () => {
			const noDefault: AgentsConfig = {
				agents: {
					claude: {
						run_configs: [
							{ name: "A", command: "claude", args: [], env: {}, is_default: false },
							{ name: "B", command: "claude", args: [], env: {}, is_default: false },
						],
					},
				},
			};

			await testInScopeAsync(async () => {
				await hydrateWith(noDefault);
				const def = store.getDefaultConfig("claude");
				expect(def?.name).toBe("A");
			});
		});

		it("returns undefined for agent with no configs", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				const def = store.getDefaultConfig("aider");
				expect(def).toBeUndefined();
			});
		});
	});

	describe("addRunConfig()", () => {
		it("adds a config and saves", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				const newConfig: AgentRunConfig = {
					name: "Test",
					command: "claude",
					args: ["--test"],
					env: {},
					is_default: false,
				};
				await store.addRunConfig("claude", newConfig);
				const configs = store.getRunConfigs("claude");
				expect(configs).toHaveLength(1);
				// First config should be auto-set as default
				expect(configs[0].is_default).toBe(true);
				expect(configs[0].name).toBe("Test");
			});
		});
	});

	it("does not drop the migration marker when Settings removes Codex bypass", async () => {
		let persisted: AgentsConfig = {
			agents: {
				codex: {
					codex_bypass_migrated: true,
					run_configs: [
						{
							name: "Default",
							command: "codex",
							args: ["--dangerously-bypass-approvals-and-sandbox"],
							env: {},
							is_default: true,
						},
					],
				},
			},
		};
		const machine = createAgentConfigsStore({
			load: async () => persisted,
			save: async (_base, config) => {
				persisted = config;
			},
		});
		await machine.hydrate();
		const config = machine.getDefaultConfig("codex")!;
		await machine.updateRunConfig("codex", 0, { ...config, args: [] });
		expect(persisted.agents.codex.codex_bypass_migrated).toBe(true);
		expect(persisted.agents.codex.run_configs[0].args).toEqual([]);
		await machine.hydrate();
		expect(machine.getDefaultConfig("codex")?.args).toEqual([]);
	});

	describe("updateRunConfig()", () => {
		it("updates a config at index", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				const updated: AgentRunConfig = {
					name: "Updated",
					command: "claude",
					args: ["--verbose"],
					env: {},
					is_default: true,
				};
				await store.updateRunConfig("claude", 0, updated);
				expect(store.getRunConfigs("claude")[0].name).toBe("Updated");
				expect(store.getRunConfigs("claude")[0].args).toEqual(["--verbose"]);
			});
		});

		it("ignores out-of-bounds index", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getRunConfigs("claude")[0].name).toBe("Default");

				const updated: AgentRunConfig = {
					name: "X",
					command: "x",
					args: [],
					env: {},
					is_default: false,
				};
				await store.updateRunConfig("claude", 99, updated);
				expect(store.getRunConfigs("claude")[0].name).toBe("Default");
				expect(store.getRunConfigs("claude")).toHaveLength(2);
			});
		});
	});

	describe("updateRunConfigEnv()", () => {
		it("persists env from entries", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.updateRunConfigEnv("claude", 0, [
					{ key: "FOO", value: "1" },
					{ key: "BAR", value: "2" },
				]);
				expect(store.getRunConfigs("claude")[0].env).toEqual({ FOO: "1", BAR: "2" });
			});
		});

		it("throws on duplicate keys rather than silently overwriting", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await expect(
					store.updateRunConfigEnv("claude", 0, [
						{ key: "FOO", value: "1" },
						{ key: "FOO", value: "2" },
					]),
				).rejects.toThrow(/Duplicate env keys.*FOO/);
				expect(store.getRunConfigs("claude")[0].env).toEqual({});
			});
		});

		it("ignores empty/whitespace keys", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.updateRunConfigEnv("claude", 0, [
					{ key: "FOO", value: "1" },
					{ key: "  ", value: "2" },
					{ key: "", value: "3" },
				]);
				expect(store.getRunConfigs("claude")[0].env).toEqual({ FOO: "1" });
			});
		});

		it("ignores out-of-bounds index", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.updateRunConfigEnv("claude", 99, [{ key: "FOO", value: "1" }]);
				expect(store.getRunConfigs("claude")[0].env).toEqual({});
			});
		});
	});

	describe("removeRunConfig()", () => {
		it("removes a config and reassigns default", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.removeRunConfig("claude", 0);
				const configs = store.getRunConfigs("claude");
				expect(configs).toHaveLength(1);
				expect(configs[0].name).toBe("Print");
				expect(configs[0].is_default).toBe(true);
			});
		});
	});

	describe("setDefaultConfig()", () => {
		it("sets a specific config as default", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.setDefaultConfig("claude", 1);
				const configs = store.getRunConfigs("claude");
				expect(configs[0].is_default).toBe(false);
				expect(configs[1].is_default).toBe(true);
			});
		});
	});

	describe("headless agent", () => {
		it("defaults to null", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				expect(store.getHeadlessAgent()).toBeNull();
			});
		});

		it("persists headless_agent from config", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {}, headless_agent: "claude" });
				expect(store.getHeadlessAgent()).toBe("claude");
			});
		});

		it("can be set to 'api' for External API mode", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				store.setHeadlessAgent("api");
				expect(store.getHeadlessAgent()).toBe("api");
			});
		});

		it("saves when headless agent changes", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				mockInvoke.mockClear();
				store.setHeadlessAgent("api");
				// setHeadlessAgent triggers a save
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({ headless_agent: "api" }),
					}),
				);
			});
		});

		it("accepts and round-trips a 'agentType:configName' composite value selecting a named run config", async () => {
			// The store must not narrow this to a plain AgentType — SmartPromptsTab's
			// grouped dropdown renders named run configs as composite values, and
			// useSmartPrompts.ts's executeHeadless parses them back apart.
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {}, headless_agent: "claude:My Config" });
				expect(store.getHeadlessAgent()).toBe("claude:My Config");

				store.setHeadlessAgent("gemini:Other Config");
				expect(store.getHeadlessAgent()).toBe("gemini:Other Config");
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({ headless_agent: "gemini:Other Config" }),
					}),
				);
			});
		});
	});

	describe("per-agent progress_tracking", () => {
		it("has no opinion until one is set, which is how the agent follows the global flag", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getProgressTracking("claude")).toBeUndefined();
			});
		});

		it("persists the override and can hand the decision back to the global flag", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				mockInvoke.mockClear();

				await store.setProgressTracking("claude", false);
				expect(store.getProgressTracking("claude")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({
								claude: expect.objectContaining({ progress_tracking: false }),
							}),
						}),
					}),
				);

				await store.setProgressTracking("claude", undefined);
				expect(store.getProgressTracking("claude")).toBeUndefined();
			});
		});
	});

	describe("intent_tab_title override", () => {
		it("defaults to undefined (use the global setting)", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				expect(store.getIntentTabTitle("claude")).toBeUndefined();
			});
		});

		it("sets and persists true", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				mockInvoke.mockClear();
				await store.setIntentTabTitle("claude", true);
				expect(store.getIntentTabTitle("claude")).toBe(true);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({ claude: expect.objectContaining({ intent_tab_title: true }) }),
						}),
					}),
				);
			});
		});

		it("sets and persists false", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				await store.setIntentTabTitle("claude", false);
				expect(store.getIntentTabTitle("claude")).toBe(false);
			});
		});

		it("resets to undefined (inherit) when set to undefined — this is the tri-state 'use global' path", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				await store.setIntentTabTitle("claude", false);
				expect(store.getIntentTabTitle("claude")).toBe(false);
				await store.setIntentTabTitle("claude", undefined);
				expect(store.getIntentTabTitle("claude")).toBeUndefined();
			});
		});
	});

	describe("suggest_followups override", () => {
		it("defaults to undefined (use the global setting)", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				expect(store.getSuggestFollowups("claude")).toBeUndefined();
			});
		});

		it("sets, persists, and can be reset to undefined", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				await store.setSuggestFollowups("claude", true);
				expect(store.getSuggestFollowups("claude")).toBe(true);

				mockInvoke.mockClear();
				await store.setSuggestFollowups("claude", undefined);
				expect(store.getSuggestFollowups("claude")).toBeUndefined();
				// The saved payload is JSON round-tripped (clone()), which drops
				// undefined-valued keys entirely rather than serializing them as null —
				// so "reset to inherit" means the key is absent, not present-as-undefined.
				const saved = mockInvoke.mock.calls[0][1] as {
					config: { agents: Record<string, { suggest_followups?: boolean }> };
				};
				expect(saved.config.agents.claude).not.toHaveProperty("suggest_followups");
			});
		});
	});

	describe("auto-retry and hook instrumentation", () => {
		it("isAutoRetryEnabled defaults to false and reflects setAutoRetry", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				expect(store.isAutoRetryEnabled("claude")).toBe(false);
				await store.setAutoRetry("claude", true);
				expect(store.isAutoRetryEnabled("claude")).toBe(true);
			});
		});

		it("syncHookInstrumentation mirrors state without saving to disk", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				mockInvoke.mockClear();
				store.syncHookInstrumentation("claude", true);
				expect(store.getHookInstrumentation("claude")).toBe(true);
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});
	});

	describe("intent_tab_title override", () => {
		it("is undefined when no override is set", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getIntentTabTitle("claude")).toBeUndefined();
			});
		});

		it("persists a per-agent override and saves to disk", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				mockInvoke.mockClear();
				await store.setIntentTabTitle("claude", false);
				expect(store.getIntentTabTitle("claude")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({
								claude: expect.objectContaining({ intent_tab_title: false }),
							}),
						}),
					}),
				);
			});
		});

		it("resets to default when set to undefined", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.setIntentTabTitle("claude", false);
				await store.setIntentTabTitle("claude", undefined);
				expect(store.getIntentTabTitle("claude")).toBeUndefined();
			});
		});

		it("creates an agent entry when none exists yet", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				await store.setIntentTabTitle("gemini", true);
				expect(store.getIntentTabTitle("gemini")).toBe(true);
			});
		});
	});

	describe("suggest_followups override", () => {
		it("is undefined when no override is set", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getSuggestFollowups("claude")).toBeUndefined();
			});
		});

		it("persists a per-agent override and saves to disk", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				mockInvoke.mockClear();
				await store.setSuggestFollowups("claude", false);
				expect(store.getSuggestFollowups("claude")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({
								claude: expect.objectContaining({ suggest_followups: false }),
							}),
						}),
					}),
				);
			});
		});

		it("resets to default when set to undefined", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.setSuggestFollowups("claude", false);
				await store.setSuggestFollowups("claude", undefined);
				expect(store.getSuggestFollowups("claude")).toBeUndefined();
			});
		});
	});

	describe("prefer_tuic_messaging override", () => {
		it("is undefined when no override is set", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getPreferTuicMessaging("claude")).toBeUndefined();
			});
		});

		it("persists a per-agent override and saves to disk", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				mockInvoke.mockClear();
				await store.setPreferTuicMessaging("claude", false);
				expect(store.getPreferTuicMessaging("claude")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({
								claude: expect.objectContaining({ prefer_tuic_messaging: false }),
							}),
						}),
					}),
				);
			});
		});

		it("resets to default when set to undefined", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.setPreferTuicMessaging("claude", false);
				await store.setPreferTuicMessaging("claude", undefined);
				expect(store.getPreferTuicMessaging("claude")).toBeUndefined();
			});
		});

		it("creates an agent entry when none exists yet", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				await store.setPreferTuicMessaging("codex", false);
				expect(store.getPreferTuicMessaging("codex")).toBe(false);
			});
		});
	});

	describe("prefer_tuic_spawning override", () => {
		it("is undefined when no override is set", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				expect(store.getPreferTuicSpawning("claude")).toBeUndefined();
			});
		});

		it("persists a per-agent override and saves to disk", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				mockInvoke.mockClear();
				await store.setPreferTuicSpawning("claude", false);
				expect(store.getPreferTuicSpawning("claude")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith(
					"save_agents_config",
					expect.objectContaining({
						config: expect.objectContaining({
							agents: expect.objectContaining({
								claude: expect.objectContaining({ prefer_tuic_spawning: false }),
							}),
						}),
					}),
				);
			});
		});

		it("resets to default when set to undefined", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.setPreferTuicSpawning("claude", false);
				await store.setPreferTuicSpawning("claude", undefined);
				expect(store.getPreferTuicSpawning("claude")).toBeUndefined();
			});
		});

		it("creates an agent entry when none exists yet", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith({ agents: {} });
				await store.setPreferTuicSpawning("codex", false);
				expect(store.getPreferTuicSpawning("codex")).toBe(false);
			});
		});

		it("is independent of prefer_tuic_messaging in both directions", async () => {
			await testInScopeAsync(async () => {
				await hydrateWith(configWithClaude());
				await store.setPreferTuicSpawning("claude", false);
				await store.setPreferTuicMessaging("claude", true);
				expect(store.getPreferTuicSpawning("claude")).toBe(false);
				expect(store.getPreferTuicMessaging("claude")).toBe(true);

				await store.setPreferTuicSpawning("claude", true);
				await store.setPreferTuicMessaging("claude", false);
				expect(store.getPreferTuicSpawning("claude")).toBe(true);
				expect(store.getPreferTuicMessaging("claude")).toBe(false);
			});
		});
	});
});
