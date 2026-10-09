import { render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { SessionFileHeader } from "../../components/SessionDiffTab/SessionFileHeader";
import type { FileReview } from "../../types/sessionDiff";

function fileGroup(overrides: Partial<FileReview> = {}): FileReview {
	return {
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		display_path: "a.ts",
		net_change: "modified",
		base_source: "backup",
		cumulative_patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 3,
		deletions: 2,
		step_indices: [0],
		drifted_from_disk: false,
		backup_available: true,
		is_binary: false,
		revision: "rev1",
		...overrides,
	};
}

describe("SessionFileHeader", () => {
	it("clicking anywhere on the header row toggles expand — not just the 12px chevron button", () => {
		const onToggleExpanded = vi.fn();
		const { container } = render(() => (
			<SessionFileHeader
				group={fileGroup()}
				stepCount={1}
				expanded={true}
				stepsOpen={false}
				onToggleExpanded={onToggleExpanded}
				onToggleStepsOpen={() => {}}
				onOpenFile={() => {}}
				onRevertFile={() => {}}
				onCopyFile={() => {}}
			/>
		));

		// Click the header's own background area (the file-stats span is inert
		// chrome, not a button/link) — today only the chevron <button> has an
		// onClick, so this should currently do nothing.
		const header = container.querySelector(".fileHeader") as HTMLElement;
		const statsArea = header.querySelector(".fileStats") as HTMLElement;
		statsArea.click();
		expect(onToggleExpanded).toHaveBeenCalledTimes(1);
	});

	it("clicking the copy, revert, or 'show N edits' action buttons does not toggle expand", () => {
		const onToggleExpanded = vi.fn();
		const onCopyFile = vi.fn();
		const onRevertFile = vi.fn();
		const onToggleStepsOpen = vi.fn();
		const { getByTitle, getByText } = render(() => (
			<SessionFileHeader
				group={fileGroup()}
				stepCount={2}
				expanded={true}
				stepsOpen={false}
				onToggleExpanded={onToggleExpanded}
				onToggleStepsOpen={onToggleStepsOpen}
				onOpenFile={() => {}}
				onRevertFile={onRevertFile}
				onCopyFile={onCopyFile}
			/>
		));

		getByTitle("Copy this file's diff").click();
		getByTitle("Revert this file to its session-start content").click();
		getByText("Show 2 edits").click();

		expect(onCopyFile).toHaveBeenCalledTimes(1);
		expect(onRevertFile).toHaveBeenCalledTimes(1);
		expect(onToggleStepsOpen).toHaveBeenCalledTimes(1);
		expect(onToggleExpanded).not.toHaveBeenCalled();
	});

	it("clicking the file path opens the file, not toggle expand", () => {
		const onToggleExpanded = vi.fn();
		const onOpenFile = vi.fn();
		const { getByText } = render(() => (
			<SessionFileHeader
				group={fileGroup()}
				stepCount={1}
				expanded={true}
				stepsOpen={false}
				onToggleExpanded={onToggleExpanded}
				onToggleStepsOpen={() => {}}
				onOpenFile={onOpenFile}
				onRevertFile={() => {}}
				onCopyFile={() => {}}
			/>
		));

		getByText("a.ts").click();
		expect(onOpenFile).toHaveBeenCalledTimes(1);
		expect(onToggleExpanded).not.toHaveBeenCalled();
	});

	it("renders the outside-repo, drifted, and unknown-base badges, plus additions/deletions", () => {
		const { getByText, getByTitle } = render(() => (
			<SessionFileHeader
				group={fileGroup({
					in_repo: false,
					drifted_from_disk: true,
					base_source: "unknown",
					additions: 4,
					deletions: 1,
				})}
				stepCount={1}
				expanded={true}
				stepsOpen={false}
				onToggleExpanded={() => {}}
				onToggleStepsOpen={() => {}}
				onOpenFile={() => {}}
				onRevertFile={() => {}}
				onCopyFile={() => {}}
			/>
		));

		expect(getByText("outside repo")).toBeTruthy();
		expect(getByText("drifted")).toBeTruthy();
		expect(getByText("unknown base")).toBeTruthy();
		expect(getByText("+4")).toBeTruthy();
		expect(getByText("-1")).toBeTruthy();
		// Revert action is hidden when the base is unknown (nothing to revert to).
		expect(() => getByTitle("Revert this file to its session-start content")).toThrow();
	});
});
