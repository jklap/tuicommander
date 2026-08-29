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
import { agentConfigsStore as store } from "../../stores/agentConfigs";
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

		it("handles hydrate failure gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("load failed"));
			const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.loaded).toBe(true);
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
});
