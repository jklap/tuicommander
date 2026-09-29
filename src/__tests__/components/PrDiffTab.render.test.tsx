// wip's PrDiffTab render tests (header, empty state, totals, mode toggle). Kept
// in their own file: they lay rows out through `installVirtualLayout`, while
// PrDiffTab.test.tsx (collapsing) mocks `@tanstack/solid-virtual` file-wide.
import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installVirtualLayout, uninstallVirtualLayout } from "../helpers/virtualLayout";

// DiffFileList renders each file through DiffViewer (@git-diff-view/solid),
// which needs a real Canvas this test environment doesn't have (see
// DiffTab.test.tsx's own note) — stubbed so these tests exercise only
// PrDiffTab's own header/empty-state/mode-toggle wiring.
vi.mock("../../components/ui/DiffViewer", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../components/ui/DiffViewer")>();
	return {
		...actual,
		DiffViewer: (props: { diff: string }) => <div data-testid="diff-stub">{props.diff}</div>,
	};
});

import { PrDiffTab } from "../../components/PrDiffTab/PrDiffTab";
import { uiStore } from "../../stores/ui";

function diffFor(path: string): string {
	return `diff --git a/${path} b/${path}\n@@ -1,1 +1,2 @@\n-old\n+new\n+extra\n`;
}

describe("PrDiffTab", () => {
	beforeEach(() => {
		installVirtualLayout();
	});
	afterEach(() => {
		uninstallVirtualLayout();
		uiStore.setDiffViewMode("split");
		// setDiffViewMode() schedules a debounced save(); cancel it so it
		// doesn't fire (and leak a timer) after the test has already ended.
		uiStore._testCancelPendingSave();
	});

	it("renders the PR number, title, and per-file rows", () => {
		const { getByText } = render(() => <PrDiffTab prNumber={42} prTitle="Fix the thing" diff={diffFor("a.ts")} />);
		expect(getByText("#42 Fix the thing")).toBeTruthy();
		expect(getByText("a.ts")).toBeTruthy();
	});

	it("shows the empty state when the diff has no files", () => {
		const { getByText, queryByText } = render(() => <PrDiffTab prNumber={1} prTitle="Empty PR" diff="" />);
		expect(getByText("No changes")).toBeTruthy();
		expect(queryByText("a.ts")).toBeNull();
	});

	it("shows the total additions and deletions across files", () => {
		const diff = diffFor("a.ts") + diffFor("b.ts");
		const { container } = render(() => <PrDiffTab prNumber={2} prTitle="Two files" diff={diff} />);
		const stats = container.querySelector(".headerStats");
		expect(stats?.textContent).toContain("2");
		expect(stats?.textContent).toContain("+4");
		expect(stats?.textContent).toContain("-2");
	});

	it("the mode-toggle buttons switch the global diff view mode", () => {
		uiStore.setDiffViewMode("split");
		const { getByTitle } = render(() => <PrDiffTab prNumber={3} prTitle="Mode toggle" diff={diffFor("a.ts")} />);

		getByTitle("Inline").click();
		expect(uiStore.state.diffViewMode).toBe("unified");

		getByTitle("Side-by-side").click();
		expect(uiStore.state.diffViewMode).toBe("split");
	});
});
