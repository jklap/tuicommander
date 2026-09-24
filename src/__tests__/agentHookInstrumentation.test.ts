import { describe, expect, it, vi } from "vitest";
import { type AgentType, HOOK_SUPPORT } from "../agents";
import { createAgentConfigsStore } from "../stores/agentConfigs";

describe("agent hook instrumentation toggle", () => {
	it("HOOK_SUPPORT gates the hook-instrumented agents (A1)", () => {
		// Adapters landed incrementally: Claude/Gemini (#048), then Grok (#051),
		// Codex (#050), OpenCode (#052) flipped on. Keep this in sync with agents.ts.
		const on: AgentType[] = ["claude", "gemini", "grok", "codex", "opencode"];
		for (const a of on) {
			expect(HOOK_SUPPORT[a]).toBe(true);
		}
		const off: AgentType[] = ["aider", "cursor", "amp", "goose", "droid", "pi", "git", "api"];
		for (const a of off) {
			expect(HOOK_SUPPORT[a]).toBe(false);
		}
	});

	it("getHookInstrumentation reflects the stored flag", () => {
		const store = createAgentConfigsStore({
			load: async () => ({ agents: { claude: { run_configs: [], hook_instrumentation: true } } }),
			save: vi.fn(),
		});
		// Seed via sync (load is async and not awaited here) then read back.
		store.syncHookInstrumentation("claude", true);
		expect(store.getHookInstrumentation("claude")).toBe(true);
		expect(store.getHookInstrumentation("gemini")).toBeUndefined();
	});

	it("syncHookInstrumentation mirrors the flag in memory without saving to disk", () => {
		const save = vi.fn();
		const store = createAgentConfigsStore({
			load: async () => ({ agents: {} }),
			save,
		});
		store.syncHookInstrumentation("claude", true);
		expect(store.getHookInstrumentation("claude")).toBe(true);
		store.syncHookInstrumentation("claude", false);
		// Off is the default, and the backend stores it as absent; the mirror matches.
		expect(store.getHookInstrumentation("claude")).toBeUndefined();
		// The Tauri command owns persistence — sync must never write to disk.
		expect(save).not.toHaveBeenCalled();
	});

	it("a later save writes no explicit default for either flag", async () => {
		// The backend stores the default as absent. A whole-file save from the
		// store must not write it back, or Settings reads it as a user override.
		const save = vi.fn();
		const store = createAgentConfigsStore({
			load: async () => ({
				agents: { claude: { run_configs: [], hook_instrumentation: true, native_status_signals: false } },
			}),
			save,
		});
		await store.hydrate();
		store.syncHookInstrumentation("claude", false);
		store.syncNativeStatusSignals("claude", true);
		expect(store.getNativeStatusSignals("claude")).toBe(true);

		await store.setAutoRetry("claude", true);
		const saved = save.mock.calls[0][0].agents.claude;
		expect(saved).not.toHaveProperty("hook_instrumentation");
		expect(saved).not.toHaveProperty("native_status_signals");
		expect(saved.auto_retry_on_error).toBe(true);
	});

	it("keeps a non-default flag as an explicit override", () => {
		const store = createAgentConfigsStore({ load: async () => ({ agents: {} }), save: vi.fn() });
		store.syncHookInstrumentation("gemini", true);
		store.syncNativeStatusSignals("claude", false);
		expect(store.state.agents.gemini.hook_instrumentation).toBe(true);
		expect(store.state.agents.claude.native_status_signals).toBe(false);
	});
});
