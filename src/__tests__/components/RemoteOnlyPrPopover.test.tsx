import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";
import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { RemoteOnlyPrPopover } from "../../components/Sidebar/RemoteOnlyPrPopover";
import { repoSettingsStore } from "../../stores/repoSettings";
import type { BranchPrStatus } from "../../types";

const { mockPollRepo } = vi.hoisted(() => ({
	mockPollRepo: vi.fn(),
}));

vi.mock("../../stores/github", () => ({
	githubStore: {
		state: { viewerLogin: null },
		pollRepo: mockPollRepo,
		getCheckSummary: vi.fn(() => null),
		getCheckDetails: vi.fn(() => []),
		loadCheckDetails: vi.fn(() => Promise.resolve()),
		getBranchPrData: vi.fn(() => mergeablePr),
	},
}));

vi.mock("../../stores/mdTabs", () => ({
	mdTabsStore: {
		addPrDiff: vi.fn(),
	},
}));

vi.mock("../../stores/repoDefaults", () => ({
	repoDefaultsStore: {
		state: { prMergeStrategy: "merge" },
	},
}));

const mergeablePr: BranchPrStatus = {
	branch: "feature/x",
	number: 42,
	title: "Feature",
	state: "OPEN",
	url: "https://github.com/org/repo/pull/42",
	additions: 10,
	deletions: 5,
	author: "alice",
	commits: 1,
	checks: { passed: 2, failed: 0, pending: 0, total: 2 },
	check_details: [],
	labels: [],
	is_draft: false,
	base_ref_name: "main",
	head_ref_oid: "abc",
	created_at: "",
	updated_at: "",
	merge_state_label: null,
	review_state_label: null,
	review_decision: "APPROVED",
	viewer_did_approve: false,
	mergeable: "MERGEABLE",
	conflict_state: "clear" as const,
	merge_state_status: "CLEAN",
	merge_commit_allowed: true,
	squash_merge_allowed: true,
	rebase_merge_allowed: true,
	unresolved_threads: 0,
	unresolved_threads_truncated: false,
};

const defaultProps = {
	prs: [mergeablePr],
	repoPath: "/repo",
	onClose: vi.fn(),
	onCheckout: vi.fn(),
};

describe("RemoteOnlyPrPopover — 405 merge method auto-fallback", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		repoSettingsStore.getOrCreate("/repo", "Repo");
		repoSettingsStore.update("/repo", { prMergeStrategy: null }); // reset to global default
	});

	const clickMerge = (container: HTMLElement) => {
		const row = container.querySelector(".remoteOnlyRow");
		fireEvent.click(row!);
		const mergeBtn = Array.from(container.querySelectorAll("button")).find((b) =>
			["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
		) as HTMLButtonElement;
		fireEvent.click(mergeBtn);
	};

	it("falls back to squash when merge is rejected with 405", async () => {
		mockInvoke
			.mockResolvedValueOnce({}) // get_pr_action_state, read on mount by the PR action buttons
			.mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed."))
			.mockResolvedValueOnce("abc123sha");

		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
		clickMerge(container);

		await waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith(
				"merge_pr_via_github",
				expect.objectContaining({ mergeMethod: "squash" }),
			);
		});
		await waitFor(() => expect(repoSettingsStore.getEffective("/repo")?.prMergeStrategy).toBe("squash"));
	});

	it("falls back to rebase when both merge and squash are rejected", async () => {
		mockInvoke
			.mockResolvedValueOnce({}) // get_pr_action_state, read on mount by the PR action buttons
			.mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed."))
			.mockRejectedValueOnce(new Error("GitHub merge failed (405): Squash merging is not allowed."))
			.mockResolvedValueOnce("abc123sha");

		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
		clickMerge(container);

		await waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith(
				"merge_pr_via_github",
				expect.objectContaining({ mergeMethod: "rebase" }),
			);
		});
		await waitFor(() => expect(repoSettingsStore.getEffective("/repo")?.prMergeStrategy).toBe("rebase"));
	});

	it("shows error when all merge methods fail", async () => {
		mockInvoke
			.mockResolvedValueOnce({}) // get_pr_action_state, read on mount by the PR action buttons
			.mockRejectedValueOnce(new Error("GitHub merge failed (405): Merge commits are not allowed."))
			.mockRejectedValueOnce(new Error("GitHub merge failed (405): Squash merging is not allowed."))
			.mockRejectedValueOnce(new Error("GitHub merge failed (405): Rebase merging is not allowed."));

		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
		clickMerge(container);

		await waitFor(() => {
			expect(container.textContent).toContain("405");
		});
	});
});

describe("RemoteOnlyPrPopover — merge button label reflects effective method", () => {
	const makePr = (overrides = {}): BranchPrStatus => ({
		...mergeablePr,
		...overrides,
	});

	const renderAndExpand = (pr: BranchPrStatus) => {
		repoSettingsStore.getOrCreate("/repo", "Repo");
		repoSettingsStore.update("/repo", { prMergeStrategy: null });
		const { container } = render(() => (
			<RemoteOnlyPrPopover prs={[pr]} repoPath="/repo" onClose={vi.fn()} onCheckout={vi.fn()} />
		));
		const row = container.querySelector(".remoteOnlyRow")!;
		fireEvent.click(row);
		return container;
	};

	it("shows Merge when merge commits are allowed", () => {
		const container = renderAndExpand(makePr({ merge_commit_allowed: true }));
		const btn = Array.from(container.querySelectorAll("button")).find((b) =>
			["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
		);
		expect(btn?.textContent).toBe("Merge");
	});

	it("shows Squash & Merge when only squash is allowed", () => {
		const container = renderAndExpand(
			makePr({ merge_commit_allowed: false, squash_merge_allowed: true, rebase_merge_allowed: false }),
		);
		const btn = Array.from(container.querySelectorAll("button")).find((b) =>
			["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
		);
		expect(btn?.textContent).toBe("Squash & Merge");
	});

	it("shows Rebase & Merge when only rebase is allowed", () => {
		const container = renderAndExpand(
			makePr({ merge_commit_allowed: false, squash_merge_allowed: false, rebase_merge_allowed: true }),
		);
		const btn = Array.from(container.querySelectorAll("button")).find((b) =>
			["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
		);
		expect(btn?.textContent).toBe("Rebase & Merge");
	});
});

describe("RemoteOnlyPrPopover — outside dismissal", () => {
	beforeEach(() => vi.clearAllMocks());

	// Catches: an invisible fixed inset:0 backdrop swallows the press (and wheel) meant for the sidebar.
	it("closes on an outside pointer press without swallowing it", () => {
		const onSidebarPress = vi.fn();
		const { container } = render(() => (
			<>
				<aside onPointerDown={onSidebarPress}>Repositories</aside>
				<RemoteOnlyPrPopover {...defaultProps} />
			</>
		));
		expect(container.querySelector(".remoteOnlyOverlay")).toBeNull();
		const event = new PointerEvent("pointerdown", { bubbles: true, cancelable: true });
		container.querySelector("aside")!.dispatchEvent(event);
		expect(defaultProps.onClose).toHaveBeenCalledOnce();
		expect(onSidebarPress).toHaveBeenCalledOnce();
		expect(event.defaultPrevented).toBe(false);
	});

	// Catches: pressing inside the popover is treated as an outside press.
	it("stays open for a press inside the popover", () => {
		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
		fireEvent.pointerDown(container.querySelector(".remoteOnlyRow")!);
		expect(defaultProps.onClose).not.toHaveBeenCalled();
	});
});

describe("RemoteOnlyPrPopover — baseRepo forwarding", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		repoSettingsStore.getOrCreate("/repo", "Repo");
		repoSettingsStore.update("/repo", { prMergeStrategy: null });
	});

	const openRow = (container: HTMLElement) => fireEvent.click(container.querySelector(".remoteOnlyRow")!);
	const clickByTooltip = (container: HTMLElement, tooltip: string) =>
		fireEvent.click(container.querySelector(`button[data-tooltip="${tooltip}"]`)!);

	it("sends the PR's base_repo with the diff request", async () => {
		mockInvoke.mockResolvedValue({ diff: "", headSha: "abc" });
		const pr = { ...mergeablePr, base_repo: "upstream/repo" };
		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} prs={[pr]} />);
		openRow(container);
		clickByTooltip(container, "View PR diff");
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("get_pr_diff", {
				repoPath: "/repo",
				prNumber: 42,
				baseRepo: "upstream/repo",
			}),
		);
	});

	it("sends no baseRepo with the diff request when the PR has none", async () => {
		mockInvoke.mockResolvedValue({ diff: "", headSha: "abc" });
		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} />);
		openRow(container);
		clickByTooltip(container, "View PR diff");
		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_pr_diff", expect.anything()));
		const call = mockInvoke.mock.calls.find((c) => c[0] === "get_pr_diff");
		expect((call![1] as { baseRepo?: string }).baseRepo).toBeUndefined();
	});

	it("sends the PR's base_repo when merging", async () => {
		mockInvoke.mockResolvedValue("sha");
		const pr = { ...mergeablePr, base_repo: "upstream/repo" };
		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} prs={[pr]} />);
		openRow(container);
		const mergeBtn = Array.from(container.querySelectorAll("button")).find((b) =>
			["Merge", "Squash & Merge", "Rebase & Merge"].includes(b.textContent ?? ""),
		) as HTMLButtonElement;
		fireEvent.click(mergeBtn);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith(
				"merge_pr_via_github",
				expect.objectContaining({ baseRepo: "upstream/repo", expectedHeadSha: "abc" }),
			),
		);
	});
});

describe("RemoteOnlyPrPopover — PRs sharing a number across repositories", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		repoSettingsStore.getOrCreate("/repo", "Repo");
		repoSettingsStore.update("/repo", { prMergeStrategy: null });
	});

	const forkPr = { ...mergeablePr, title: "Fork PR", branch: "feature/fork", base_repo: "fork/repo" };
	const upstreamPr = { ...mergeablePr, title: "Upstream PR", branch: "feature/up", base_repo: "upstream/repo" };

	// Catches: dismissing fork #42 also hiding upstream #42.
	it("dismissing one PR does not hide the other with the same number", () => {
		mockInvoke.mockResolvedValue({});
		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} prs={[forkPr, upstreamPr]} />);
		fireEvent.click(container.querySelectorAll(".remoteOnlyRow")[0]);
		fireEvent.click(container.querySelector('button[data-tooltip="Hide this PR from view"]')!);
		const rows = container.querySelectorAll(".remoteOnlyRow");
		expect(rows).toHaveLength(1);
		expect(rows[0].textContent).toContain("Upstream PR");
	});

	// Catches: one PR's in-flight diff load disabling the other PR's Diff button.
	it("a diff load in flight on one PR leaves the same-numbered PR's button enabled", async () => {
		let finishDiff: (v: unknown) => void = () => {};
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "get_pr_diff"
				? new Promise((resolve) => {
						finishDiff = resolve;
					})
				: Promise.resolve({}),
		);
		const { container } = render(() => <RemoteOnlyPrPopover {...defaultProps} prs={[forkPr, upstreamPr]} />);
		const diffButton = () => container.querySelector('button[data-tooltip="View PR diff"]') as HTMLButtonElement;
		fireEvent.click(container.querySelectorAll(".remoteOnlyRow")[0]);
		fireEvent.click(diffButton());
		await waitFor(() => expect(diffButton().disabled).toBe(true));
		fireEvent.click(container.querySelectorAll(".remoteOnlyRow")[1]);
		await waitFor(() => expect(container.textContent).toContain("Upstream PR"));
		expect(diffButton().disabled).toBe(false);
		finishDiff({ diff: "", headSha: "abc" });
	});
});
