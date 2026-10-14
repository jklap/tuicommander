import { createRoot } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useOpenMarkdownPublication } from "../../hooks/useOpenMarkdownPublication";
import { mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";

const { rpc, environment } = vi.hoisted(() => ({
	rpc: vi.fn(async (_command: string, _args?: Record<string, unknown>) => {}),
	environment: { desktop: true },
}));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc,
	isTauri: () => environment.desktop,
}));
let dispose: (() => void) | undefined;
function mount() {
	createRoot((stop) => {
		dispose = stop;
		useOpenMarkdownPublication();
	});
}
beforeEach(() => {
	vi.useFakeTimers();
	environment.desktop = true;
	mdTabsStore.clearAll();
	rpc.mockClear();
});
afterEach(() => {
	dispose?.();
	dispose = undefined;
	mdTabsStore.clearAll();
	repositoriesStore._testCancelPendingSave();
	vi.useRealTimers();
});

// Catches: desktop open/close never reaching the backend, or plugin HTML/content leaking into the snapshot.
it("publishes desktop file opens and closes as metadata over IPC", async () => {
	mount();
	const id = mdTabsStore.add("/repo", "docs/guide.md");
	mdTabsStore.addVirtual("Virtual", "test:virtual");
	await vi.advanceTimersByTimeAsync(50);
	expect(rpc.mock.calls).toEqual([
		["sync_open_markdown_tabs", { tabs: [{ id, repoPath: "/repo", filePath: "docs/guide.md", fsRoot: "/repo" }] }],
	]);
	mdTabsStore.remove(id);
	await vi.advanceTimersByTimeAsync(50);
	expect(rpc.mock.calls.at(-1)).toEqual(["sync_open_markdown_tabs", { tabs: [] }]);
});

// Catches: late completion of an old snapshot reintroducing a tab after close.
it("serializes publications and sends the latest close after an in-flight open", async () => {
	let finish: (() => void) | undefined;
	rpc.mockImplementationOnce(
		() =>
			new Promise<void>((resolve) => {
				finish = resolve;
			}),
	);
	mount();
	const id = mdTabsStore.add("/repo", "guide.md");
	await vi.advanceTimersByTimeAsync(50);
	mdTabsStore.remove(id);
	await vi.advanceTimersByTimeAsync(50);
	expect(rpc).toHaveBeenCalledTimes(1);
	finish?.();
	await vi.advanceTimersByTimeAsync(1000);
	expect(rpc.mock.calls.at(-1)).toEqual(["sync_open_markdown_tabs", { tabs: [] }]);
});

// Catches: a phone or browser document overwriting the desktop's file list with its empty local store.
it("does not publish from browser or mobile documents", async () => {
	environment.desktop = false;
	mount();
	mdTabsStore.add("/repo", "guide.md");
	await vi.advanceTimersByTimeAsync(1000);
	expect(rpc).not.toHaveBeenCalled();
});
