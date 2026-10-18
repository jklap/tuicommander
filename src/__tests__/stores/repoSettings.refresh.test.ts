import { expect, it, vi } from "vitest";
import "../mocks/tauri";
import { settingsRefresh } from "../../services/settingsRefresh";
import { repoSettingsStore } from "../../stores/repoSettings";
import { toastsStore } from "../../stores/toasts";
import { mockInvoke } from "../mocks/tauri";

it("omitted defaults adopt peer changes — catches null and empty-map defaults masquerading as unsaved intent", async () => {
	mockInvoke.mockResolvedValue({ repos: { "/repo": { path: "/repo", display_name: "Repo" } } });
	await repoSettingsStore.hydrate();
	let apply: ((value: unknown) => void) | undefined;
	vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
		if (reader.section === "repositories") apply = reader.apply;
		return () => {};
	});
	const notice = vi.spyOn(toastsStore, "add").mockReturnValue(0);
	const stop = repoSettingsStore.startRemoteSync();
	try {
		apply!({
			repos: {
				"/repo": { path: "/repo", display_name: "Repo", mcp_upstreams: ["peer"], branch_labels: { main: "Peer" } },
			},
		});
		expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toEqual(["peer"]);
		expect(repoSettingsStore.get("/repo")?.branchLabels).toEqual({ main: "Peer" });
		expect(notice).not.toHaveBeenCalled();
		apply!({ repos: { "/repo": { path: "/repo", display_name: "Repo" } } });
		expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toBeNull();
		expect(repoSettingsStore.get("/repo")?.branchLabels).toEqual({});
		expect(mockInvoke.mock.calls.some(([command]) => command === "save_repo_settings")).toBe(false);
	} finally {
		stop();
		vi.restoreAllMocks();
	}
});

// Catches: normalizing clean omitted defaults also discarding genuine unsaved repository intent.
it("dirty repository fields survive while untouched peer fields advance", async () => {
	mockInvoke.mockReset().mockResolvedValue({ repos: { "/repo": { path: "/repo", display_name: "Repo" } } });
	await repoSettingsStore.hydrate();
	let apply: ((value: unknown) => void) | undefined;
	vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
		if (reader.section === "repositories") apply = reader.apply;
		return () => {};
	});
	const notice = vi.spyOn(toastsStore, "add").mockReturnValue(0);
	const stop = repoSettingsStore.startRemoteSync();
	try {
		repoSettingsStore.update("/repo", { mcpUpstreams: ["draft"] });
		apply!({ repos: { "/repo": { path: "/repo", display_name: "Peer name", mcp_upstreams: ["peer"] } } });
		expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toEqual(["draft"]);
		expect(repoSettingsStore.get("/repo")?.displayName).toBe("Peer name");
		expect(notice).toHaveBeenCalledOnce();
		await Promise.resolve();
	} finally {
		stop();
		vi.restoreAllMocks();
	}
});
