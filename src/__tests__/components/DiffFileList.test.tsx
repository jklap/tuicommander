import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installVirtualLayout, uninstallVirtualLayout } from "../helpers/virtualLayout";

// DiffFileList renders each file through DiffViewer (@git-diff-view/solid),
// which needs a real Canvas this test environment doesn't have (see
// DiffTab.test.tsx's own note) — stubbed so these tests exercise only
// DiffFileList's row/collapse/key logic, not the diff renderer itself.
vi.mock("../../components/ui/DiffViewer", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../components/ui/DiffViewer")>();
	return {
		...actual,
		DiffViewer: (props: { diff: string }) => <div data-testid="diff-stub">{props.diff}</div>,
	};
});

import { DiffFileList, sectionToRawDiff } from "../../components/shared/DiffFileList";
import type { DiffListNavHandle } from "../../components/shared/diffListNav";
import { type DiffFileSection, parseDiffFiles } from "../../components/ui/DiffViewer";

function section(path: string, marker: string): DiffFileSection {
	const raw = `diff --git a/${path} b/${path}\n@@ -1,1 +1,1 @@\n-old\n+${marker}`;
	const [s] = parseDiffFiles(raw);
	return s;
}

/** The diff-stub's whole patch is one text node, so an exact-text query
 *  never matches a marker embedded inside it — match by substring instead. */
function hasMarker(marker: string) {
	return (content: string) => content.includes(marker);
}

describe("sectionToRawDiff", () => {
	it("reconstructs a file section's raw diff from its parsed lines", () => {
		const raw = "diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new";
		const [parsedSection] = parseDiffFiles(raw);
		expect(sectionToRawDiff(parsedSection)).toBe(raw);
	});
});

describe("DiffFileList", () => {
	beforeEach(() => {
		installVirtualLayout();
	});
	afterEach(() => {
		uninstallVirtualLayout();
	});

	it("renders the provided header above the list", () => {
		const files = parseDiffFiles("diff --git a/a.ts b/a.ts\n@@ -1 +1 @@\n-x\n+y");
		const { getByText } = render(() => <DiffFileList files={files} mode="unified" header={<div>HEADER</div>} />);
		expect(getByText("HEADER")).toBeTruthy();
	});

	it("mounts without throwing when there are no files", () => {
		const { container } = render(() => <DiffFileList files={[]} mode="unified" />);
		expect(container).toBeTruthy();
	});

	it("replacing all files shows the new files' content, not the previous files'", () => {
		const [files, setFiles] = createSignal([section("a.ts", "MARKER_A"), section("b.ts", "MARKER_B")]);
		const { getByText, queryByText } = render(() => <DiffFileList files={files()} mode="unified" />);
		expect(getByText("a.ts")).toBeTruthy();
		expect(getByText("b.ts")).toBeTruthy();

		setFiles([section("c.ts", "MARKER_C"), section("d.ts", "MARKER_D")]);
		expect(queryByText("a.ts")).toBeNull();
		expect(queryByText("b.ts")).toBeNull();
		expect(getByText("c.ts")).toBeTruthy();
		expect(getByText("d.ts")).toBeTruthy();
	});

	it("collapse state follows the FILE, not the list slot it happened to occupy", () => {
		const [files, setFiles] = createSignal([section("a.ts", "MARKER_A"), section("b.ts", "MARKER_B")]);
		const { container, getByText, queryByText } = render(() => <DiffFileList files={files()} mode="unified" />);

		// Collapse b.ts (currently at index 1).
		const bHeader = getByText("b.ts").closest('[role="button"]') as HTMLElement;
		bHeader.click();
		expect(queryByText(hasMarker("MARKER_B"))).toBeNull();
		expect(getByText(hasMarker("MARKER_A"))).toBeTruthy();

		// Swap order — b.ts is now at index 0, a.ts is now at index 1. Same
		// length, so the virtualizer keeps the same two rendered slots; only
		// the *local* `createSignal(false)` inside each slot's `FileSection`
		// decides collapse today, which is bound to the slot, not the file.
		setFiles([section("b.ts", "MARKER_B"), section("a.ts", "MARKER_A")]);

		// b.ts must still be collapsed (the user's action was on that file).
		expect(queryByText(hasMarker("MARKER_B"))).toBeNull();
		// a.ts was never touched — it must still be expanded.
		expect(getByText(hasMarker("MARKER_A"))).toBeTruthy();
		void container;
	});

	it("a staged and an unstaged copy of the same path render as two separate rows, not one", () => {
		// BranchDiffScrollView concatenates a staged diff and an unstaged diff;
		// a partially-staged file produces two DiffFileSection entries that
		// share the same `path`. `getItemKey: (i) => props.files[i]?.path`
		// collides for both, so the virtualizer sees a duplicate key.
		const dup = [section("dup.ts", "STAGED_HALF"), section("dup.ts", "UNSTAGED_HALF")];
		const { getAllByText, getAllByTestId } = render(() => <DiffFileList files={dup} mode="unified" />);

		expect(getAllByText("dup.ts").length).toBe(2);
		const stubs = getAllByTestId("diff-stub").map((el) => el.textContent ?? "");
		expect(stubs.some(hasMarker("STAGED_HALF"))).toBe(true);
		expect(stubs.some(hasMarker("UNSTAGED_HALF"))).toBe(true);
	});

	it("exposes a nav handle whose currentIndex starts at the first row and scrollToIndex drives the container's scrollTo", () => {
		let handle: DiffListNavHandle | undefined;
		const scrollToSpy = vi.spyOn(HTMLElement.prototype, "scrollTo").mockImplementation(() => {});
		const files = [section("a.ts", "MARKER_A"), section("b.ts", "MARKER_B"), section("c.ts", "MARKER_C")];
		render(() => <DiffFileList files={files} mode="unified" ref={(h) => (handle = h)} />);
		expect(handle?.currentIndex()).toBe(0);
		handle?.scrollToIndex(2);
		expect(scrollToSpy).toHaveBeenCalled();
		scrollToSpy.mockRestore();
	});
});
