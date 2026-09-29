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
const h = vi.hoisted(() => ({ getDiff: vi.fn(), openFileAction: vi.fn() }));
vi.mock("../../components/shared/DiffFileList", () => ({
	DiffFileList: (props: {
		files: Array<{ path: string; additions: number; deletions: number }>;
		mode: string;
		header?: JSX.Element;
		onOpenFile?: (path: string) => void;
	}) => (
		<div data-testid="mock-file-list" data-mode={props.mode}>
			{props.header}
			{props.files.map((f) => (
				<button type="button" data-testid={`file-${f.path}`} onClick={() => props.onOpenFile?.(f.path)}>
					{f.path}
				</button>
			))}
		</div>
	),
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

		expect(h.getDiff).toHaveBeenCalledWith(REPO);
		expect(h.getDiff).toHaveBeenCalledWith(REPO, "staged");
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
});
