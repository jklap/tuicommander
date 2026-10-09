import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installVirtualLayout, uninstallVirtualLayout } from "../helpers/virtualLayout";

// DiffViewer needs a real Canvas that happy-dom doesn't provide (see
// DiffTab.test.tsx's note) — stubbed so these tests exercise only
// SessionDiffList's own row rendering/virtualization, not the diff renderer.
vi.mock("../../components/ui/DiffViewer", () => ({
	DiffViewer: (props: { diff: string }) => <div data-testid="diff-stub">{props.diff}</div>,
}));

import type { SessionRow, StepRowData } from "../../components/SessionDiffTab/buildRows";
import { SessionDiffList, type SessionDiffListProps } from "../../components/SessionDiffTab/SessionDiffList";
import type { DiffListNavHandle } from "../../components/shared/diffListNav";
import type { EditStep, FileReview } from "../../types/sessionDiff";

function fileGroup(overrides: Partial<FileReview> = {}): FileReview {
	return {
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		display_path: "a.ts",
		net_change: "modified",
		base_source: "backup",
		cumulative_patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 1,
		deletions: 1,
		step_indices: [],
		drifted_from_disk: false,
		backup_available: true,
		is_binary: false,
		revision: "rev1",
		...overrides,
	};
}

function editStep(overrides: Partial<EditStep> = {}): EditStep {
	return {
		step_index: 0,
		tool_use_id: "toolu_1",
		timestamp: null,
		kind: "edit",
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 1,
		deletions: 1,
		is_sidechain: false,
		agent_name: null,
		user_modified: false,
		replace_all: false,
		turn_index: 0,
		turn_started_at: null,
		prompt_preview: null,
		agent_id: null,
		agent_display_name: null,
		...overrides,
	};
}

function stepEntry(overrides: Partial<EditStep> = {}, entryOverrides: Partial<StepRowData> = {}): StepRowData {
	return {
		step: editStep(overrides),
		collapsed: false,
		prevSameFileStepIndex: null,
		nextSameFileStepIndex: null,
		...entryOverrides,
	};
}

function stepRow(overrides: Partial<EditStep> = {}, entryOverrides: Partial<StepRowData> = {}): SessionRow {
	return { kind: "step", ...stepEntry(overrides, entryOverrides) };
}

function fileRow(
	overrides: Partial<FileReview> = {},
	rowOverrides: Partial<{ expanded: boolean; stepsOpen: boolean; steps: StepRowData[] }> = {},
): SessionRow {
	return {
		kind: "file",
		group: fileGroup(overrides),
		steps: rowOverrides.steps ?? [],
		expanded: rowOverrides.expanded ?? true,
		stepsOpen: rowOverrides.stepsOpen ?? false,
	};
}

const noop = () => {};
const baseProps: Omit<SessionDiffListProps, "rows"> = {
	mode: "unified",
	onOpenFile: noop,
	onOpenAtLine: noop,
	onRevertStep: noop,
	onRevertFile: noop,
	onCopyStep: noop,
	onCopyFile: noop,
	onToggleExpanded: noop,
	onToggleStepsOpen: noop,
	onToggleStepCollapsed: noop,
	onJumpToStep: noop,
};

describe("SessionDiffList", () => {
	beforeEach(() => {
		installVirtualLayout();
	});
	afterEach(() => {
		uninstallVirtualLayout();
	});

	it("replacing all rows shows the new rows' content, not the previous rows' (mode/session-switch bug)", () => {
		const [rows, setRows] = createSignal<SessionRow[]>([
			fileRow({ abs_path: "/repo/a.ts", display_path: "a.ts" }),
			fileRow({ abs_path: "/repo/b.ts", display_path: "b.ts" }),
		]);
		const { getByText, queryByText } = render(() => <SessionDiffList rows={rows()} {...baseProps} />);
		expect(getByText("a.ts")).toBeTruthy();
		expect(getByText("b.ts")).toBeTruthy();

		setRows([
			fileRow({ abs_path: "/repo/c.ts", display_path: "c.ts" }),
			fileRow({ abs_path: "/repo/d.ts", display_path: "d.ts" }),
		]);

		// This is the bug at SessionDiffList.tsx:66-68: `const row =
		// props.rows[vi.index]` is read once, so a position already on screen
		// never re-reads new data — a.ts/b.ts stay on screen instead of
		// c.ts/d.ts appearing.
		expect(queryByText("a.ts")).toBeNull();
		expect(queryByText("b.ts")).toBeNull();
		expect(getByText("c.ts")).toBeTruthy();
		expect(getByText("d.ts")).toBeTruthy();
	});

	it("updating a row's data under the same key updates the rendered content without remounting the row's DOM node", () => {
		const [rows, setRows] = createSignal<SessionRow[]>([fileRow({ abs_path: "/repo/a.ts", additions: 1 })]);
		const { container, getByText } = render(() => <SessionDiffList rows={rows()} {...baseProps} />);

		expect(getByText("+1")).toBeTruthy();
		const wrapperBefore = container.querySelector('[data-index="0"]');
		expect(wrapperBefore).toBeTruthy();

		// Same abs_path (same getItemKey), different additions count.
		setRows([fileRow({ abs_path: "/repo/a.ts", additions: 5 })]);

		const wrapperAfter = container.querySelector('[data-index="0"]');
		// The virtualizer keeps the same slot for the same key — the row
		// content itself must update in place, not stay frozen at its first
		// value. (Currently fails: `row` is captured once at creation.)
		expect(getByText("+5")).toBeTruthy();
		// The DOM node identity should be preserved (no needless remount) —
		// true both before and after the real fix.
		expect(wrapperAfter).toBe(wrapperBefore);
	});

	it("a different row landing in the same slot key does get fresh content (sanity check on the key itself)", () => {
		const [rows, setRows] = createSignal<SessionRow[]>([fileRow({ abs_path: "/repo/a.ts", display_path: "a.ts" })]);
		const { getByText, queryByText } = render(() => <SessionDiffList rows={rows()} {...baseProps} />);
		expect(getByText("a.ts")).toBeTruthy();

		setRows([fileRow({ abs_path: "/repo/z.ts", display_path: "z.ts" })]);
		expect(queryByText("a.ts")).toBeNull();
		expect(getByText("z.ts")).toBeTruthy();
	});

	it("exposes a nav handle whose currentIndex tracks the virtualizer's own first-visible row", () => {
		let handle: DiffListNavHandle | undefined;
		const rows = [fileRow({ abs_path: "/repo/a.ts" }), fileRow({ abs_path: "/repo/b.ts" })];
		render(() => <SessionDiffList rows={rows} {...baseProps} ref={(h) => (handle = h)} />);
		expect(handle).toBeTruthy();
		expect(handle?.currentIndex()).toBe(0);
	});

	it("scrollToIndex on the nav handle drives the scroll container's own scrollTo (happy-dom doesn't reflect scrollTop back from it)", () => {
		let handle: DiffListNavHandle | undefined;
		const scrollToSpy = vi.spyOn(HTMLElement.prototype, "scrollTo").mockImplementation(() => {});
		const rows = Array.from({ length: 5 }, (_, i) => fileRow({ abs_path: `/repo/${i}.ts` }));
		render(() => <SessionDiffList rows={rows} {...baseProps} ref={(h) => (handle = h)} />);
		handle?.scrollToIndex(4);
		expect(scrollToSpy).toHaveBeenCalled();
		scrollToSpy.mockRestore();
	});

	it("stepRow's collapsed/prevSameFileStepIndex/nextSameFileStepIndex reach StepCard's ^/v buttons", () => {
		const jumpTargets: number[] = [];
		const rows = [stepRow({ step_index: 5 }, { prevSameFileStepIndex: 2, nextSameFileStepIndex: 9 })];
		const { getByTitle } = render(() => (
			<SessionDiffList rows={rows} {...baseProps} onJumpToStep={(i) => jumpTargets.push(i)} />
		));
		getByTitle("Jump to the earlier change to this file").click();
		getByTitle("Jump to the later change to this file").click();
		expect(jumpTargets).toEqual([2, 9]);
	});

	// NOTE: the sticky-header-offset bug (shared/diffFileList.module.css's
	// `--diff-header-height` is unconditionally 35px on `.container`, which
	// assumes a consumer that always renders a sticky 35px summary bar above
	// the list — SessionDiffList never passes a `header` prop, so its file
	// headers stick 35px below the top instead of at it) is NOT unit-testable
	// here: CSS Modules are not actually injected as real stylesheets under
	// this vitest/happy-dom setup (confirmed empirically — `document
	// .querySelectorAll("style")` is always empty after a render), so
	// `getComputedStyle` never reflects real cascaded values. Per this
	// project's own convention (src/AGENTS.md "Visual" section), this class of
	// bug is verified with a screenshot during implementation, not a test.
});
