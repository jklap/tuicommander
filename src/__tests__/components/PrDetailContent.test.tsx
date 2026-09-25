import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

const mockGithubStore = vi.hoisted(() => ({
	getBranchPrData: vi.fn(),
	getCheckSummary: vi.fn(() => null),
	getCheckDetails: vi.fn(() => []),
	loadCheckDetails: vi.fn(() => Promise.resolve()),
}));
const mockRpc = vi.hoisted(() => vi.fn());

vi.mock("../../stores/github", () => ({ githubStore: mockGithubStore }));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { PrDetailContent } from "../../components/PrDetailPopover/PrDetailContent";

const basePr = {
	number: 42,
	state: "OPEN",
	branch: "feature",
	base_ref_name: "main",
	author: "boss",
	commits: 1,
	additions: 10,
	deletions: 2,
	labels: [],
	mergeable: "MERGEABLE",
	merge_state_label: null,
	review_state_label: null,
	created_at: null,
	updated_at: null,
};

const cleanFile = (path: string) => ({ path, summary: "changed stuff", findings: [] });

// prReviewStore is a module singleton keyed by repo+PR, so each test uses its
// own repo path — otherwise a later test reads the previous one's result. It is
// read into a const first: Solid props are getters, so calling nextRepo()
// inside the JSX would hand the component a different repo on every read.
let repoSeq = 0;
const nextRepo = () => `/repo-${++repoSeq}`;

describe("PrDetailContent — ego review result metadata", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockGithubStore.getBranchPrData.mockReturnValue(basePr);
		mockGithubStore.getCheckSummary.mockReturnValue(null);
		mockGithubStore.getCheckDetails.mockReturnValue([]);
	});

	it("shows reviewed-files count and names ego when the review returns no findings", async () => {
		mockInvoke.mockResolvedValue({
			repo_path: "/repo",
			pr_number: 42,
			head_sha: "abc",
			summary: "Two clean refactors, nothing risky.",
			files: [cleanFile("src/a.ts"), cleanFile("src/b.ts")],
		});
		const repo = nextRepo();
		const { getByText, findByText } = render(() => <PrDetailContent repoPath={repo} branch="feature" />);
		fireEvent.click(getByText("Run"));

		// Proof-of-work line: file count + who ran it, not just a bare "No findings".
		await findByText(/2 files reviewed/);
		expect(getByText(/by ego/)).toBeTruthy();
		expect(getByText("Two clean refactors, nothing risky.")).toBeTruthy();
		expect(getByText("No findings")).toBeTruthy();
	});

	it("fetches and renders a failed CircleCI check log without opening its URL", async () => {
		mockGithubStore.getCheckDetails.mockReturnValue([
			{ context: "ci/circleci: test", state: "failure", html_url: "https://circleci.com/gh/acme/widget/42" },
		]);
		mockRpc.mockResolvedValue("\u001b[31mfailed step\u001b[0m");
		const repo = nextRepo();
		const { getByText, findByText } = render(() => <PrDetailContent repoPath={repo} branch="feature" />);
		fireEvent.click(getByText("Log"));
		expect(mockRpc).toHaveBeenCalledWith("fetch_ci_failure_logs", { repoPath: repo, branch: "feature" });
		expect(await findByText("failed step")).toBeTruthy();
	});

	it("does not pluralise a single reviewed file", async () => {
		mockInvoke.mockResolvedValue({
			repo_path: "/repo",
			pr_number: 42,
			head_sha: "abc",
			summary: "Reviewed 1 file",
			files: [cleanFile("pnpm-lock.yaml")],
		});
		const repo = nextRepo();
		const { getByText, findByText } = render(() => <PrDetailContent repoPath={repo} branch="feature" />);
		fireEvent.click(getByText("Run"));

		await findByText(/1 file reviewed/);
	});

	it("shows ego's own sentence and no metadata when ego is not reachable", async () => {
		// Verbatim from `acp::oneshot::ask`: the reason is actionable, "review
		// failed" is not, and an empty finding list would be a silent lie.
		mockInvoke.mockRejectedValue(new Error("ego could not run this turn: no ego executable is configured"));
		const repo = nextRepo();
		const { getByText, findByText, queryByText } = render(() => <PrDetailContent repoPath={repo} branch="feature" />);
		fireEvent.click(getByText("Run"));

		await findByText(/no ego executable is configured/);
		expect(queryByText("No findings")).toBeNull();
		expect(queryByText(/reviewed/)).toBeNull();
	});

	it("still lists findings with checkboxes when the review has findings", async () => {
		const fileWithFinding = {
			path: "src/bug.ts",
			summary: "changed stuff",
			findings: [
				{
					path: "src/bug.ts",
					line: 7,
					hunk: null,
					severity: "bug",
					confidence: 0.9,
					message: "Null deref on empty input",
				},
			],
		};
		mockInvoke.mockResolvedValue({
			repo_path: "/repo",
			pr_number: 42,
			head_sha: "abc",
			summary: "One bug found.",
			files: [fileWithFinding],
		});
		const repo = nextRepo();
		const { getByText, findByText, queryByText } = render(() => <PrDetailContent repoPath={repo} branch="feature" />);
		fireEvent.click(getByText("Run"));

		await findByText("Null deref on empty input");
		await waitFor(() => expect(queryByText("No findings")).toBeNull());
		expect(getByText("Post review")).toBeTruthy();
	});

	it("leaves a file-level finding visible but unselectable — GitHub needs a line", async () => {
		mockInvoke.mockResolvedValue({
			repo_path: "/repo",
			pr_number: 42,
			head_sha: "abc",
			summary: "One file-level note.",
			files: [
				{
					path: "src/wide.ts",
					summary: "changed stuff",
					findings: [
						{
							path: "src/wide.ts",
							line: null,
							hunk: null,
							severity: "risk",
							confidence: 0.8,
							message: "This module has no tests at all",
						},
					],
				},
			],
		});
		const repo = nextRepo();
		const { container, getByText, findByText } = render(() => <PrDetailContent repoPath={repo} branch="feature" />);
		fireEvent.click(getByText("Run"));

		await findByText("This module has no tests at all");
		const checkbox = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
		expect(checkbox.disabled).toBe(true);
		// Pre-selected but not postable, so the post button stays out of reach.
		expect((getByText("Post review") as HTMLButtonElement).disabled).toBe(true);
	});
});
