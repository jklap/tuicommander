import { render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { usePluginRuntime } from "../../hooks/usePluginRuntime";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import type { PluginHost, StateChangeEvent } from "../../plugins/types";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";

vi.mock("../../plugins", () => ({ initPlugins: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(() => {}),
}));

afterEach(() => {
	pluginRegistry.clear();
	repositoriesStore._testCancelPendingSave();
	terminalsStore._testCancelPendingTimers();
});

it("runtime delivers branch shell and awaiting transitions", () => {
	const path = "/plugin-runtime";
	repositoriesStore.add({ path, displayName: "Runtime" });
	repositoriesStore.setWorkspace(path, "main", { branchName: "main" });
	repositoriesStore.setActiveWorkspace(path, "main");
	repositoriesStore.setActive(path);
	const terminalId = terminalsStore.add({
		sessionId: "runtime-session",
		name: "Runtime",
		cwd: path,
		fontSize: 14,
		awaitingInput: null,
	});
	let host!: PluginHost;
	pluginRegistry.register({
		id: "observer",
		onload(api) {
			host = api;
		},
		onunload() {},
	});
	const events: StateChangeEvent[] = [];
	host.onStateChange((event) => events.push(event));
	const { unmount } = render(() => {
		usePluginRuntime();
		return null;
	});

	repositoriesStore.setWorkspace(path, "main", { branchName: "renamed" });
	terminalsStore.update(terminalId, { shellState: "busy" });
	terminalsStore.update(terminalId, { awaitingInput: "question" });

	expect(events).toEqual([
		{ type: "branch-changed", sessionId: null, terminalId: "", detail: "renamed" },
		{ type: "shell-state-changed", sessionId: "runtime-session", terminalId, detail: "busy" },
		{ type: "awaiting-input-changed", sessionId: "runtime-session", terminalId, detail: "question" },
	]);
	events.length = 0;
	terminalsStore.update(terminalId, { shellState: "busy", name: "Renamed" });
	expect(events).toEqual([]);
	terminalsStore.update(terminalId, { awaitingInput: null, shellState: "idle" });
	expect(events.map((event) => event.type)).toEqual(["shell-state-changed", "awaiting-input-changed"]);
	expect(events[1].detail).toBeUndefined();
	unmount();
	events.length = 0;
	terminalsStore.update(terminalId, { shellState: "busy" });
	expect(events).toEqual([]);
	terminalsStore.remove(terminalId);
	repositoriesStore.remove(path);
});
