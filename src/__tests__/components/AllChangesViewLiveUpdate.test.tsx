import { cleanup, render, waitFor } from "@solidjs/testing-library";
import type { JSX } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AllChangesView } from "../../components/DiffTab/AllChangesView";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

// DiffFileList renders each file through DiffViewer, which needs a real Canvas this
// environment lacks. Stubbed so these tests exercise only AllChangesView's own
// live-update logic: flash of on-screen changes, "Refresh (N)" for off-screen ones,
// "New content below", the nav handle it exposes, and the diff options it fetches with.
const h = vi.hoisted(() => ({
	getDiff: vi.fn(),
	getInfo: vi.fn(),
	listBaseRefOptions: vi.fn(),
	navScrollToIndex: vi.fn(),
	navCurrentIndex: 0,
	navVisibleIndices: new Set<number>() as ReadonlySet<number>,
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ getDiff: h.getDiff, getInfo: h.getInfo, listBaseRefOptions: h.listBaseRefOptions }),
}));
vi.mock("../../components/shared/DiffFileList", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/shared/DiffFileList")>()),
	DiffFileList: (props: {
		files: Array<{ path: string }>;
		flashKeys?: ReadonlySet<string>;
		header?: JSX.Element;
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
			// Like the real handle: derived live from the (possibly longer) fresh list.
			rowCount: () => props.files.length,
			visibleIndices: () => h.navVisibleIndices,
		});
		return (
			<div data-testid="mock-file-list" data-flash-keys={[...(props.flashKeys ?? [])].join(",")}>
				{props.header}
				{props.files.map((f) => (
					<section data-testid={`file-${f.path}`}>{f.path}</section>
				))}
			</div>
		);
	},
}));

const REPO = "/live-repo";

function diffFor(path: string, body = "+new"): string {
	return `diff --git a/${path} b/${path}\n@@ -1,1 +1,1 @@\n-old\n${body}\n`;
}

/** The diff text the (mocked) backend returns next. */
let nextDiff: string;

beforeEach(() => {
	vi.clearAllMocks();
	h.navCurrentIndex = 0;
	h.navVisibleIndices = new Set();
	nextDiff = diffFor("a.txt");
	h.getDiff.mockImplementation(async () => nextDiff);
	// On the base branch: the default scope is "uncommitted" (scope "head").
	h.getInfo.mockResolvedValue({ branch: "main" });
	h.listBaseRefOptions.mockResolvedValue([{ name: "main", kind: "local", is_default: true }]);
	mockInvoke.mockImplementation(async (cmd: string) => (cmd === "get_branch_base" ? null : undefined));
});
afterEach(cleanup);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("AllChangesView live updates", () => {
	// Catches: a revision bump replacing the list under the reviewer's eyes with no cue.
	it("flashes a changed file that is on screen, then fades it after ~1.5s", async () => {
		const view = render(() => <AllChangesView repoPath={REPO} />);
		await waitFor(() => expect(view.getByTestId("file-a.txt")).toBeTruthy());
		h.navVisibleIndices = new Set([0]);

		vi.useFakeTimers();
		try {
			nextDiff = diffFor("a.txt", "+something else entirely");
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(0);

			expect(view.getByTestId("mock-file-list").dataset.flashKeys).toBe("a.txt#0");
			await vi.advanceTimersByTimeAsync(1600);
			expect(view.getByTestId("mock-file-list").dataset.flashKeys).toBe("");
		} finally {
			vi.useRealTimers();
		}
	});

	// Catches: an off-screen file's content changing while the reviewer reads another one.
	it("holds a changed off-screen file behind a 'Refresh (N)' pill until clicked", async () => {
		const view = render(() => <AllChangesView repoPath={REPO} />);
		await waitFor(() => expect(view.getByTestId("file-a.txt")).toBeTruthy());
		h.navVisibleIndices = new Set();

		nextDiff = diffFor("a.txt", "+something else");
		repositoriesStore.bumpRevision(REPO);
		const pill = await waitFor(() => view.getByText("Refresh (1)"));

		expect(view.getByTestId("mock-file-list").dataset.flashKeys).toBe("");
		pill.click();
		await waitFor(() => expect(view.queryByText("Refresh (1)")).toBeNull());
	});

	// Catches: a file appended below the viewport going unnoticed.
	it("offers 'New content below' when a file is appended off screen, scrolling to it on click", async () => {
		nextDiff = [diffFor("a.txt"), diffFor("b.txt"), diffFor("c.txt")].join("");
		const view = render(() => <AllChangesView repoPath={REPO} />);
		await waitFor(() => expect(view.getByTestId("file-c.txt")).toBeTruthy());
		h.navCurrentIndex = 0;

		nextDiff = [diffFor("a.txt"), diffFor("b.txt"), diffFor("c.txt"), diffFor("d.txt")].join("");
		repositoriesStore.bumpRevision(REPO);
		const pill = await waitFor(() => view.getByText("New content below"));

		pill.click();
		expect(h.navScrollToIndex).toHaveBeenCalledWith(3, { align: "end" }); // 4 files -> last index 3
	});

	// Catches: DiffTab's `<`/`>` file toolbar never getting the list's nav handle.
	it("hands the nav handle to the caller's ref", async () => {
		const ref = vi.fn();
		render(() => <AllChangesView repoPath={REPO} ref={ref} />);
		await waitFor(() => expect(ref).toHaveBeenCalled());
		expect(typeof ref.mock.calls[0][0].scrollToIndex).toBe("function");
	});

	// Catches: whitespace/case diff options never reaching the backend, or not reloading on change.
	it("fetches with the active whitespace/case options and refetches when they change", async () => {
		render(() => <AllChangesView repoPath={REPO} />);
		await waitFor(() => expect(h.getDiff).toHaveBeenCalled());
		expect(h.getDiff).toHaveBeenLastCalledWith(REPO, "head", {
			ignoreLeadingWs: false,
			ignoreTrailingWs: false,
			ignoreWsAmount: false,
			ignoreCase: false,
		});
		const initial = h.getDiff.mock.calls.length;

		settingsStore.setDiffIgnoreCase(true);
		try {
			await settle();
			expect(h.getDiff.mock.calls.length).toBeGreaterThan(initial);
			expect(h.getDiff).toHaveBeenLastCalledWith(REPO, "head", {
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
