import { render } from "@solidjs/testing-library";
import type { JSX } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// DiffFileList renders each file through DiffViewer (@git-diff-view/solid),
// which needs a real Canvas this test environment doesn't have (see
// DiffTab.test.tsx's note). Stubbed here so these tests exercise only
// BranchDiffScrollView's own logic: fetch/join order, zero-change filtering,
// loading/error states, its diffGen stale-response guard, and onOpenFile
// wiring — not the shared list/viewer's own rendering (covered by
// DiffFileList.test.tsx / DiffViewer.test.tsx).
const h = vi.hoisted(() => ({
	getDiff: vi.fn(),
	openFileAction: vi.fn(),
	navScrollToIndex: vi.fn(),
	navCurrentIndex: 0,
	navVisibleIndices: new Set<number>() as ReadonlySet<number>,
}));
vi.mock("../../components/shared/DiffFileList", async (importOriginal) => ({
	// Real implementation kept: BranchDiffScrollView's own live-update
	// classification calls this directly (not through the mocked component)
	// to fingerprint each file's content.
	sectionToRawDiff: (await importOriginal<typeof import("../../components/shared/DiffFileList")>()).sectionToRawDiff,
	DiffFileList: (props: {
		files: Array<{ path: string; additions: number; deletions: number }>;
		mode: string;
		wrap?: boolean;
		maxLines?: number;
		flashKeys?: ReadonlySet<string>;
		header?: JSX.Element;
		onOpenFile?: (path: string) => void;
		ref?: (handle: {
			scrollToIndex: (i: number, opts?: { align?: string }) => void;
			currentIndex: () => number;
			rowCount: () => number;
			visibleIndices: () => ReadonlySet<number>;
		}) => void;
	}) => {
		props.ref?.({
			scrollToIndex: h.navScrollToIndex,
			currentIndex: () => h.navCurrentIndex,
			// Real `DiffFileList`'s own handle derives this from `props.files.length`
			// live — matched here rather than a static test double, since
			// BranchDiffScrollView's "is the reviewer near the bottom" check reads
			// it AFTER applying a fresh (possibly longer) file list.
			rowCount: () => props.files.length,
			visibleIndices: () => h.navVisibleIndices,
		});
		return (
			<div
				data-testid="mock-file-list"
				data-mode={props.mode}
				data-wrap={String(props.wrap ?? false)}
				data-max-lines={String(props.maxLines ?? 0)}
				data-flash-keys={[...(props.flashKeys ?? [])].join(",")}
			>
				{props.header}
				{props.files.map((f) => (
					<button type="button" data-testid={`file-${f.path}`} onClick={() => props.onOpenFile?.(f.path)}>
						{f.path}
					</button>
				))}
			</div>
		);
	},
	// Real implementation kept: BranchDiffScrollView calls this directly (not
	// through the mocked component) to prune its own collapse-state set.
	fileRowKeys: (files: Array<{ path: string }>) => {
		const counts = new Map<string, number>();
		return files.map((f) => {
			const path = f.path ?? "";
			const n = counts.get(path) ?? 0;
			counts.set(path, n + 1);
			return `${path}#${n}`;
		});
	},
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ getDiff: h.getDiff }),
}));
vi.mock("../../utils/filePreview", () => ({
	openFileAction: h.openFileAction,
}));

import { BranchDiffScrollView } from "../../components/DiffTab/BranchDiffScrollView";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { uiStore } from "../../stores/ui";

const REPO = "/repo";
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function diffFor(path: string): string {
	return `diff --git a/${path} b/${path}\n@@ -1,1 +1,1 @@\n-old\n+new\n`;
}

const ZERO_CHANGE_DIFF =
	"diff --git a/renamed.txt b/renamed2.txt\nsimilarity index 100%\nrename from renamed.txt\nrename to renamed2.txt\n";

describe("BranchDiffScrollView", () => {
	beforeEach(() => {
		h.getDiff.mockReset();
		h.openFileAction.mockReset();
		h.navScrollToIndex.mockReset();
		h.navCurrentIndex = 0;
		h.navVisibleIndices = new Set();
	});
	afterEach(() => {
		// setDiffViewMode() schedules a debounced save(); cancel it so it
		// doesn't fire (and leak a timer) after the test has already ended.
		uiStore._testCancelPendingSave();
	});

	it("fetches staged and unstaged, joining staged first then unstaged", async () => {
		h.getDiff.mockImplementation((_path: string, scope?: string) =>
			Promise.resolve(scope === "staged" ? diffFor("staged.txt") : diffFor("unstaged.txt")),
		);
		render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();

		const noOptions = { ignoreLeadingWs: false, ignoreTrailingWs: false, ignoreWsAmount: false, ignoreCase: false };
		expect(h.getDiff).toHaveBeenCalledWith(REPO, undefined, noOptions);
		expect(h.getDiff).toHaveBeenCalledWith(REPO, "staged", noOptions);
		const calls = h.getDiff.mock.calls;
		// staged.txt's own file entry must render before unstaged.txt's.
		const list = document.querySelectorAll("button[data-testid^='file-']");
		expect(Array.from(list).map((el) => el.textContent)).toEqual(["staged.txt", "unstaged.txt"]);
		expect(calls.length).toBeGreaterThanOrEqual(2);
	});

	it("filters out files with zero additions and zero deletions", async () => {
		h.getDiff.mockImplementation((_path: string, scope?: string) =>
			Promise.resolve(scope === "staged" ? ZERO_CHANGE_DIFF : diffFor("real.txt")),
		);
		const { queryByTestId } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();

		expect(queryByTestId("file-renamed2.txt")).toBeNull();
		expect(queryByTestId("file-real.txt")).not.toBeNull();
	});

	it("shows a loading state, then the empty state when there are no changes", async () => {
		h.getDiff.mockResolvedValue("");
		const { getByText } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		expect(getByText(/Loading diff/)).toBeTruthy();
		await settle();
		expect(getByText(/No uncommitted changes/)).toBeTruthy();
	});

	it("shows an error state when a fetch rejects", async () => {
		h.getDiff.mockRejectedValue(new Error("boom"));
		const { getByText } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();
		expect(getByText(/Error:/)).toBeTruthy();
	});

	it("a stale earlier fetch never overwrites a newer one (diffGen guard)", async () => {
		let resolveFirst: (v: string) => void = () => {};
		let call = 0;
		h.getDiff.mockImplementation((_path: string, scope?: string) => {
			if (scope === "staged") return Promise.resolve("");
			call += 1;
			if (call === 1) {
				return new Promise((resolve) => {
					resolveFirst = resolve;
				});
			}
			return Promise.resolve(diffFor("second.txt"));
		});
		const { queryByTestId } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();

		repositoriesStore.bumpRevision(REPO);
		await settle();
		resolveFirst(diffFor("first-stale.txt"));
		await settle();

		expect(queryByTestId("file-second.txt")).not.toBeNull();
		expect(queryByTestId("file-first-stale.txt")).toBeNull();
	});

	it("onOpenFile opens the file via openFileAction with the repo path", async () => {
		h.getDiff.mockImplementation((_path: string, scope?: string) =>
			Promise.resolve(scope === "staged" ? "" : diffFor("clickme.txt")),
		);
		const { getByTestId } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();

		getByTestId("file-clickme.txt").click();
		expect(h.openFileAction).toHaveBeenCalledWith("clickme.txt", REPO);
	});

	it("passes 'unified' straight through to the underlying DiffFileList", async () => {
		// BranchDiffScrollView takes `mode` as an explicit prop now (DiffTab.tsx
		// owns/computes it) instead of reading the shared uiStore.diffViewMode
		// itself — DiffViewMode no longer has a "scroll" value to map at all.
		h.getDiff.mockImplementation((_path: string, scope?: string) =>
			Promise.resolve(scope === "staged" ? "" : diffFor("a.txt")),
		);
		const { getByTestId } = render(() => <BranchDiffScrollView repoPath={REPO} mode="unified" />);
		await settle();
		expect(getByTestId("mock-file-list").dataset.mode).toBe("unified");
	});

	it("passes 'split' straight through to the underlying DiffFileList", async () => {
		h.getDiff.mockImplementation((_path: string, scope?: string) =>
			Promise.resolve(scope === "staged" ? "" : diffFor("a.txt")),
		);
		const { getByTestId } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();
		expect(getByTestId("mock-file-list").dataset.mode).toBe("split");
	});

	it("the summary header totals additions and deletions across all files", async () => {
		h.getDiff.mockImplementation((_path: string, scope?: string) =>
			Promise.resolve(
				scope === "staged"
					? "diff --git a/one.txt b/one.txt\n@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n"
					: "diff --git a/two.txt b/two.txt\n@@ -1,1 +1,1 @@\n-x\n+y\n",
			),
		);
		const { container } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
		await settle();

		// one.txt: +2/-1, two.txt: +1/-1 → totals +3/-2, across 2 files.
		const stats = container.querySelector(".headerStats");
		expect(stats?.textContent).toContain("2");
		expect(stats?.textContent).toContain("+3");
		expect(stats?.textContent).toContain("-2");
	});

	describe("live updates", () => {
		it("a changed file that's currently visible flashes on a revision bump, fading after ~1.5s", async () => {
			vi.useFakeTimers();
			try {
				h.getDiff.mockImplementation((_path: string, scope?: string) =>
					Promise.resolve(scope === "staged" ? "" : diffFor("a.txt")),
				);
				const { getByTestId } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
				await vi.advanceTimersByTimeAsync(0);
				h.navVisibleIndices = new Set([0]); // the only file is on screen

				h.getDiff.mockImplementation((_path: string, scope?: string) =>
					Promise.resolve(
						scope === "staged" ? "" : "diff --git a/a.txt b/a.txt\n@@ -1,1 +1,1 @@\n-old\n+something else entirely\n",
					),
				);
				repositoriesStore.bumpRevision(REPO);
				await vi.advanceTimersByTimeAsync(0);

				expect(getByTestId("mock-file-list").dataset.flashKeys).toBe("a.txt#0");
				await vi.advanceTimersByTimeAsync(1600);
				expect(getByTestId("mock-file-list").dataset.flashKeys).toBe("");
			} finally {
				vi.useRealTimers();
			}
		});

		it("a changed file that's off-screen is held behind a 'Refresh (N)' pill until clicked", async () => {
			h.getDiff.mockImplementation((_path: string, scope?: string) =>
				Promise.resolve(scope === "staged" ? "" : diffFor("a.txt")),
			);
			const { getByText, queryByText, getByTestId } = render(() => (
				<BranchDiffScrollView repoPath={REPO} mode="split" />
			));
			await settle();
			h.navVisibleIndices = new Set(); // off-screen

			h.getDiff.mockImplementation((_path: string, scope?: string) =>
				Promise.resolve(
					scope === "staged" ? "" : "diff --git a/a.txt b/a.txt\n@@ -1,1 +1,1 @@\n-old\n+something else\n",
				),
			);
			repositoriesStore.bumpRevision(REPO);
			await settle();

			// Held back — the original content is still what's rendered.
			expect(getByTestId("file-a.txt")).toBeTruthy();
			expect(getByTestId("mock-file-list").dataset.flashKeys).toBe("");
			const refreshPill = getByText("Refresh (1)");
			refreshPill.click();
			await settle();
			expect(queryByText("Refresh (1)")).toBeNull();
		});

		it("a new file appended below the current scroll position shows 'New content below', which scrolls to it on click", async () => {
			// `rowCount()` mirrors the real component's own `props.files.length`
			// (see the mock above), so "near the bottom" is evaluated against the
			// list's length AFTER the new file lands — a reviewer scrolled to the
			// very top of a 3-file list is clearly not near the bottom of the
			// 4-file list the appended file makes it.
			h.getDiff.mockImplementation((_path: string, scope?: string) =>
				Promise.resolve(scope === "staged" ? "" : [diffFor("a.txt"), diffFor("b.txt"), diffFor("c.txt")].join("")),
			);
			const { getByText } = render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
			await settle();
			h.navCurrentIndex = 0;

			h.getDiff.mockImplementation((_path: string, scope?: string) =>
				Promise.resolve(
					scope === "staged" ? "" : [diffFor("a.txt"), diffFor("b.txt"), diffFor("c.txt"), diffFor("d.txt")].join(""),
				),
			);
			repositoriesStore.bumpRevision(REPO);
			await settle();

			const pill = getByText("New content below");
			pill.click();
			expect(h.navScrollToIndex).toHaveBeenCalledWith(3, { align: "end" }); // 4 files -> last index 3
		});

		it("re-fetches with the active whitespace/case options, and refetches again when they change", async () => {
			h.getDiff.mockImplementation((_path: string, scope?: string) =>
				Promise.resolve(scope === "staged" ? "" : diffFor("a.txt")),
			);
			render(() => <BranchDiffScrollView repoPath={REPO} mode="split" />);
			await settle();
			const initial = h.getDiff.mock.calls.length;

			settingsStore.setDiffIgnoreCase(true);
			try {
				await settle();
				expect(h.getDiff.mock.calls.length).toBeGreaterThan(initial);
				expect(h.getDiff).toHaveBeenLastCalledWith(REPO, "staged", {
					ignoreLeadingWs: false,
					ignoreTrailingWs: false,
					ignoreWsAmount: false,
					ignoreCase: true,
				});
			} finally {
				settingsStore.setDiffIgnoreCase(false);
				settingsStore._testCancelPendingSave();
			}
		});
	});
});
