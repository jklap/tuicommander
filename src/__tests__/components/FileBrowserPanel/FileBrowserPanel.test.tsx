import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ContentMatch, ContentSearchBatch, DirEntry } from "../../../types/fs";

const { mockInvoke, mockListen, listeners } = vi.hoisted(() => {
	const listeners = new Map<string, Array<(payload: unknown) => void>>();
	return {
		listeners,
		mockInvoke: vi.fn(),
		mockListen: vi.fn((event: string, cb: (e: { payload: unknown }) => void) => {
			const wrapped = (payload: unknown) => cb({ payload });
			const arr = listeners.get(event) ?? [];
			arr.push(wrapped);
			listeners.set(event, arr);
			return Promise.resolve(() => {
				const current = listeners.get(event) ?? [];
				const idx = current.indexOf(wrapped);
				if (idx !== -1) current.splice(idx, 1);
			});
		}),
	};
});

vi.mock("../../../invoke", async (importOriginal) => ({
	...(await importOriginal<Record<string, unknown>>()),
	invoke: mockInvoke,
	listen: mockListen,
}));

const emitEvent = (event: string, payload: unknown) => {
	for (const handler of [...(listeners.get(event) ?? [])]) handler(payload);
};

const { FileBrowserPanel } = await import("../../../components/FileBrowserPanel/FileBrowserPanel");
const { uiStore } = await import("../../../stores/ui");

const dir = (name: string, path = name): DirEntry => ({
	name,
	path,
	is_dir: true,
	size: 0,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
});

const file = (name: string, path = name): DirEntry => ({
	name,
	path,
	is_dir: false,
	size: 10,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
});

const match = (path: string, line: number): ContentMatch => ({
	path,
	line_number: line,
	line_text: `hit on line ${line}`,
	match_start: 0,
	match_end: 3,
});

const batch = (matches: ContentMatch[], isFinal: boolean, searchId: string): ContentSearchBatch => ({
	search_id: searchId,
	matches,
	is_final: isFinal,
	files_searched: 1,
	files_skipped: 0,
	truncated: false,
	repos_pending: 0,
	repos_searched: 0,
});

/** `list_directory` responses keyed by `${repoPath}|${subdir}`. */
let listings = new Map<string, DirEntry[]>();
/** Every `list_directory` call, in order. */
let listCalls: Array<{ repoPath: string; subdir: string }> = [];

// Installed once, never reset: uiStore persists its prefs on a debounced timer,
// so a write can still land after a test finished — with no implementation it
// would return undefined and the store's `.catch` would throw.
mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
	if (cmd === "list_directory") {
		const repoPath = String(args?.repoPath);
		const subdir = String(args?.subdir);
		listCalls.push({ repoPath, subdir });
		return Promise.resolve(listings.get(`${repoPath}|${subdir}`) ?? []);
	}
	return Promise.resolve(undefined);
});

beforeEach(() => {
	listings = new Map();
	listCalls = [];
	listeners.clear();
	mockInvoke.mockClear();
	uiStore.setFileBrowserViewMode("flat");
});

// The view-mode toggle persists prefs on a 500 ms debounce; let that timer fire
// before teardown so it does not leak into the next test file.
afterEach(async () => {
	uiStore.setFileBrowserViewMode("flat");
	await new Promise((resolve) => setTimeout(resolve, 600));
});

const countListCalls = (repoPath: string, subdir: string) =>
	listCalls.filter((c) => c.repoPath === repoPath && c.subdir === subdir).length;

const clickTreeViewButton = (container: HTMLElement) => {
	const btn = container.querySelector('button[title="Tree view"]') as HTMLButtonElement;
	fireEvent.click(btn);
};

const clickListViewButton = (container: HTMLElement) => {
	const btn = container.querySelector('button[title="List view"]') as HTMLButtonElement;
	fireEvent.click(btn);
};

/** Click the row whose name label is `name`. */
const clickRow = (container: HTMLElement, name: string) => {
	const label = Array.from(container.querySelectorAll(".entryName")).find((el) => el.textContent === name);
	if (!label) throw new Error(`row not found: ${name}`);
	fireEvent.click(label.parentElement as HTMLElement);
};

describe("FileBrowserPanel tree cache", () => {
	it("clears cached children and expanded dirs when the repo changes", async () => {
		listings.set("/repoA|.", [dir("src")]);
		listings.set("/repoA|src", [file("from-repo-a.ts", "src/from-repo-a.ts")]);
		listings.set("/repoB|.", [dir("src")]);
		listings.set("/repoB|src", [file("from-repo-b.ts", "src/from-repo-b.ts")]);

		const [repoPath, setRepoPath] = createSignal("/repoA");
		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath={repoPath()} onClose={() => {}} onFileOpen={() => {}} />
		));

		clickTreeViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());

		clickRow(container, "src");
		await waitFor(() => expect(queryByText("from-repo-a.ts")).not.toBeNull());

		setRepoPath("/repoB");
		await waitFor(() => expect(countListCalls("/repoB", ".")).toBeGreaterThan(0));

		// repoB's tree must not render repoA's children, and nothing may stay expanded.
		expect(queryByText("from-repo-a.ts")).toBeNull();
		expect(countListCalls("/repoB", "src")).toBe(0);
	});

	it("invalidates the cached children of the directory a dir-changed event names", async () => {
		listings.set("/repo|.", [dir("src")]);
		listings.set("/repo|src", [file("a.ts", "src/a.ts")]);

		const { container, queryByText } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		clickTreeViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());

		// Expand, then collapse: children are now cached under the relative key "src".
		clickRow(container, "src");
		await waitFor(() => expect(countListCalls("/repo", "src")).toBe(1));
		clickRow(container, "src");

		// Browse into src in flat view so the dir watcher covers /repo/src.
		clickListViewButton(container);
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		clickRow(container, "src");
		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("start_dir_watcher", { path: "/repo/src" }));

		emitEvent("dir-changed", { dir_path: "/repo/src" });

		// Back to the tree: the invalidated subtree must be fetched again on expand.
		clickTreeViewButton(container);
		await waitFor(() => expect(container.querySelector(".breadcrumb")).toBeNull());
		await waitFor(() => expect(queryByText("src")).not.toBeNull());
		const before = countListCalls("/repo", "src");
		clickRow(container, "src");
		await waitFor(() => expect(countListCalls("/repo", "src")).toBe(before + 1));
	});
});

describe("FileBrowserPanel streamed content search", () => {
	/** The correlation id the panel sent with its `search_content` invoke. */
	const sentSearchId = (): string | null => {
		const call = [...mockInvoke.mock.calls].reverse().find(([cmd]) => cmd === "search_content");
		return (call?.[1] as { searchId?: string } | undefined)?.searchId ?? null;
	};

	const startContentSearch = async (container: HTMLElement): Promise<string> => {
		const modeToggle = container.querySelector('button[title="Switch to content search"]') as HTMLButtonElement;
		fireEvent.click(modeToggle);
		const input = container.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "hit" } });
		await waitFor(() => expect(sentSearchId()).not.toBeNull(), { timeout: 2000 });
		return sentSearchId() as string;
	};

	it("keeps existing group and match rows across streamed batches", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);

		emitEvent("content-search-batch", batch([match("a.ts", 1), match("a.ts", 2)], false, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentGroup").length).toBe(1));

		const firstGroup = container.querySelector(".contentGroup") as HTMLElement;
		const firstMatchRows = Array.from(container.querySelectorAll(".contentMatch"));
		expect(firstMatchRows.length).toBe(2);

		// A later batch adds a match to the same file plus a new file.
		emitEvent("content-search-batch", batch([match("a.ts", 3), match("b.ts", 9)], true, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentGroup").length).toBe(2));

		// The already-rendered group and its rows must be the very same DOM nodes.
		expect(container.querySelector(".contentGroup")).toBe(firstGroup);
		const rowsAfter = Array.from(container.querySelectorAll(".contentMatch"));
		expect(rowsAfter.slice(0, 2)).toEqual(firstMatchRows);
		expect(rowsAfter.length).toBe(4);
	});

	it("counts matches and files across batches in the status bar", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);

		emitEvent("content-search-batch", batch([match("a.ts", 1), match("a.ts", 2)], false, searchId));
		emitEvent("content-search-batch", batch([match("a.ts", 3), match("b.ts", 9)], true, searchId));

		await waitFor(() => expect(container.querySelector(".searchStatus")?.textContent).toBe("4 matches in 2 files"));
		expect(container.querySelector(".contentGroupCount")?.textContent).toBe("3 matches");
	});

	/// `content-search-batch` is one global event with three panels listening. The
	/// backend cancels the previous search when a new one starts, but the listeners
	/// are not mutually exclusive — so the command palette's batches used to append
	/// to this panel's list and flip its spinner off on someone else's `is_final`.
	it("ignores batches belonging to another panel's search", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);
		emitEvent("content-search-batch", batch([match("a.ts", 1)], false, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentMatch").length).toBe(1));

		// The command palette's search answers while this one is still streaming.
		emitEvent("content-search-batch", batch([match("other.ts", 7), match("other.ts", 8)], true, "cs-someone-else"));

		expect(container.querySelectorAll(".contentMatch").length).toBe(1);
		// Its `is_final` must not stop this panel's spinner either.
		expect(container.querySelector(".searchStatus")?.textContent).toBe("Searching…");

		// …and this panel's own final batch still lands, and does stop it.
		emitEvent("content-search-batch", batch([match("a.ts", 2)], true, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentMatch").length).toBe(2));
		expect(container.querySelector(".searchStatus")?.textContent).toBe("2 matches in 1 file");
	});

	/// Starting any other content search cancels this one — the backend has a
	/// single cancellation slot for the whole process. The cancelled search still
	/// emits an empty final batch under its own id (`dispatch_content_batches` in
	/// `fs.rs`), and that terminator is the only thing that can stop this
	/// spinner: the search that cancelled it carries a different `search_id` and
	/// is dropped above. Without it the panel spins for the life of the window.
	it("stops searching on the empty final batch a cancelled search sends", async () => {
		const { container } = render(() => (
			<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
		));

		const searchId = await startContentSearch(container);
		emitEvent("content-search-batch", batch([match("a.ts", 1)], false, searchId));
		await waitFor(() => expect(container.querySelectorAll(".contentMatch").length).toBe(1));

		// Cancelled mid-stream: no further matches, only the terminator.
		emitEvent("content-search-batch", batch([], true, searchId));

		await waitFor(() => expect(container.querySelector(".searchStatus")?.textContent).toBe("1 match in 1 file"));
		// What it did find before being cut short stays on screen.
		expect(container.querySelectorAll(".contentMatch").length).toBe(1);
	});
});
