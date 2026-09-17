import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ChangesTab } from "../../components/GitPanel/ChangesTab";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { mockInvoke } from "../mocks/tauri";

// Phase 5 of the indicator customization work: `WorkingTreeStatus.conflicted` was always
// fetched by the backend but silently dropped on the frontend — this is the "free win" that
// wires it up. See the customization plan's Phase 5 item 4.
describe("ChangesTab conflicts banner", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		settingsStore.setShowGitState(true);
	});

	afterEach(() => {
		vi.runOnlyPendingTimers();
		vi.useRealTimers();
		settingsStore.setShowGitState(true);
	});

	it("shows a conflicts banner and one row per unmerged file", async () => {
		mockInvoke.mockResolvedValue({
			staged: [],
			unstaged: [],
			untracked: [],
			conflicted: [
				{ path: "src/conflict.rs", status: "UU" },
				{ path: "src/other.rs", status: "AA" },
			],
		});
		const { container, queryByText } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(container.querySelector(".conflictsBanner")).not.toBeNull());

		expect(queryByText("2 conflicted files")).not.toBeNull();
		expect(container.querySelectorAll(".conflictEntry")).toHaveLength(2);
		// Path is split across separate dir/basename spans (see splitPath), so
		// match the full path via textContent rather than a single text node.
		expect(container.textContent).toContain("src/conflict.rs");
		expect(container.textContent).toContain("src/other.rs");
	});

	it("does not show the conflicts banner when there are no conflicts", async () => {
		mockInvoke.mockResolvedValue({ staged: [], unstaged: [], untracked: [], conflicted: [] });
		const { container } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
		expect(container.querySelector(".conflictsBanner")).toBeNull();
	});

	it("hides the conflicts banner when showGitState is off, even with conflicts present", async () => {
		settingsStore.setShowGitState(false);
		mockInvoke.mockResolvedValue({
			staged: [],
			unstaged: [],
			untracked: [],
			conflicted: [{ path: "src/conflict.rs", status: "UU" }],
		});
		const { container } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
		expect(container.querySelector(".conflictsBanner")).toBeNull();
	});

	it("does not show 'No changes' when the only change is a conflict", async () => {
		mockInvoke.mockResolvedValue({
			staged: [],
			unstaged: [],
			untracked: [],
			conflicted: [{ path: "src/conflict.rs", status: "UU" }],
		});
		const { container } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(container.querySelector(".conflictsBanner")).not.toBeNull());
		expect(container.querySelector(".empty")).toBeNull();
	});

	// Edge case: `conflicted` is typed as required on WorkingTreeStatus, but a response from
	// an older cached payload shape, a hand-rolled mock elsewhere, or a future backend that
	// omits it entirely (rather than sending `[]`) should not crash the tab — it must degrade
	// to "no conflicts", not throw on `.map` of `undefined`.
	it("does not crash when the backend response omits `conflicted` entirely", async () => {
		mockInvoke.mockResolvedValue({ staged: [], unstaged: [], untracked: [] });
		const { container } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
		expect(container.querySelector(".conflictsBanner")).toBeNull();
		expect(container.querySelector(".empty")).not.toBeNull();
	});

	// Edge case found in code review: when showGitState is off AND the working tree's only
	// change is a conflict (no staged/unstaged), the old empty-state condition
	// (`conflicted().length === 0`) suppressed "No changes" while the conflicts banner was
	// also suppressed by the toggle — the tab body went blank with no explanation at all.
	it("shows 'No changes' (not a blank body) when showGitState is off and the only change is a conflict", async () => {
		settingsStore.setShowGitState(false);
		mockInvoke.mockResolvedValue({
			staged: [],
			unstaged: [],
			untracked: [],
			conflicted: [{ path: "src/conflict.rs", status: "UU" }],
		});
		const { container } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
		expect(container.querySelector(".conflictsBanner")).toBeNull();
		expect(container.querySelector(".empty")).not.toBeNull();
	});

	it("respects an icon override for the conflicts banner", async () => {
		settingsStore.setIndicatorIcon("gitState.conflicts", "diamond");
		mockInvoke.mockResolvedValue({
			staged: [],
			unstaged: [],
			untracked: [],
			conflicted: [{ path: "src/conflict.rs", status: "UU" }],
		});
		const { container } = render(() => <ChangesTab repoPath="/repo" onOpenDiff={vi.fn()} />);

		await waitFor(() => expect(container.querySelector(".conflictsBanner")).not.toBeNull());
		const path = container.querySelector(".conflictsIcon path");
		expect(path?.getAttribute("d")).toBe("M8 1.2 14.8 8 8 14.8 1.2 8z");
		settingsStore.resetAllIndicators();
	});
});
