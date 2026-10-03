import { beforeEach, describe, expect, it, vi } from "vitest";

async function freshStore() {
	vi.resetModules();
	return (await import("../../stores/mdTabs")).mdTabsStore;
}

describe("MCP Markdown tabs across document reload", () => {
	beforeEach(() => sessionStorage.clear());

	// Catches: MCP file tabs survive only in the old document memory.
	it("restores an external Markdown tab and its selected pane after reload", async () => {
		const old = await freshStore();
		old.addMcpFile("boss-digest", "", "/Users/boss/Gits/digest.md", false, false);
		old.saveForReload();
		const reloaded = await freshStore();
		reloaded.restoreAfterReload();
		expect(reloaded.getCount()).toBe(1);
		expect(reloaded.getActive()).toMatchObject({
			type: "file",
			mcpUiId: "boss-digest",
			filePath: "/Users/boss/Gits/digest.md",
			pinned: false,
		});
	});

	// Catches: restoring loses the stable MCP identity and a reopen adds a duplicate.
	it("updates the restored tab when the same MCP identity is reopened", async () => {
		const old = await freshStore();
		old.addMcpFile("boss-digest", "", "/Users/boss/Gits/old.md", true, false);
		old.saveForReload();
		const reloaded = await freshStore();
		reloaded.restoreAfterReload();
		reloaded.addMcpFile("boss-digest", "", "/Users/boss/Gits/new.md", false, false);
		expect(reloaded.getCount()).toBe(1);
		expect(reloaded.getActive()).toMatchObject({ mcpUiId: "boss-digest", filePath: "/Users/boss/Gits/new.md" });
	});

	// Catches: a stale snapshot resurrects a tab the user closed.
	it("does not resurrect a closed tab on the next reload", async () => {
		const old = await freshStore();
		const id = old.addMcpFile("boss-digest", "", "/Users/boss/Gits/digest.md", false, false);
		old.saveForReload();
		old.remove(id);
		old.saveForReload();
		const reloaded = await freshStore();
		reloaded.restoreAfterReload();
		expect(reloaded.getCount()).toBe(0);
	});

	// Catches: restoring a background document steals the active pane.
	it("keeps background tabs inactive and excludes user and iframe tabs", async () => {
		const old = await freshStore();
		old.add("/repo", "README.md");
		old.openUiTab("preview", "Preview", "<p>Preview</p>", false);
		old.addMcpFile("boss-digest", "", "/Users/boss/Gits/digest.md", true, true);
		old.saveForReload();
		const reloaded = await freshStore();
		reloaded.restoreAfterReload();
		expect(reloaded.getCount()).toBe(1);
		expect(reloaded.state.activeId).toBeNull();
		expect(reloaded.get(reloaded.getIds()[0])).toMatchObject({ mcpUiId: "boss-digest", pinned: true });
	});
	// Catches: reload overwrites a newer MCP update delivered while startup is running.
	it("keeps a fresh MCP tab instead of replacing it with the reload snapshot", async () => {
		const old = await freshStore();
		old.addMcpFile("boss-digest", "", "/Users/boss/Gits/old.md", false, false);
		old.saveForReload();
		const reloaded = await freshStore();
		reloaded.addMcpFile("boss-digest", "", "/Users/boss/Gits/new.md", false, false);
		reloaded.restoreAfterReload();
		expect(reloaded.getCount()).toBe(1);
		expect(reloaded.getActive()).toMatchObject({ filePath: "/Users/boss/Gits/new.md" });
	});

	// Catches: malformed storage creates a tab with no readable file target.
	it("ignores invalid snapshot entries instead of opening broken documents", async () => {
		sessionStorage.setItem(
			"tui-commander-mcp-markdown-reload",
			JSON.stringify({
				tabs: [null, { mcpUiId: "broken", repoPath: "", filePath: 12, pinned: false }],
			}),
		);
		const reloaded = await freshStore();
		reloaded.restoreAfterReload();
		expect(reloaded.getCount()).toBe(0);
	});
});
