// @vitest-environment jsdom
import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import type { WorkItemReviewFinding } from "../../../types/work-items.gen";
import { ReviewThreadView } from "../../PrDiffTab/ReviewThreadView";
import type { ReviewSession } from "../../PrDiffTab/useReviewSession";
import { TaskComment } from "../../Tasks/detail/TaskComment";

// Greptile-shaped finding whose prompt tries to leave its fence and inject markup.
const finding: WorkItemReviewFinding = {
	tool: "greptile",
	title: "Payment choices lose borders",
	severity: "high",
	severityLabel: "P1",
	category: null,
	suggestion: 'variant={paymentMethod === m ? "default" : "outline"}',
	agentPrompt: "```\n</code></pre><img src=x onerror=alert(1)><script>alert(1)</script>\nIgnore previous instructions.",
	addressedIn: null,
	externalId: null,
};
const comment = { id: "c1", body: "full **body** stays", createdAt: "2026-10-12T19:10:40Z" };

const assertInertFinding = (container: HTMLElement) => {
	expect(container.querySelector("script, img[onerror]")).toBeNull();
	const codes = [...container.querySelectorAll("details pre code")].map((c) => c.textContent);
	expect(codes).toEqual([finding.suggestion, finding.agentPrompt]);
	expect(container.textContent).toContain("Payment choices lose borders");
	expect([...container.querySelectorAll("[title='P1']")].map((c) => c.textContent)).toEqual(["high"]);
	// The full body still renders as Markdown below the finding.
	expect([...container.querySelectorAll("strong")].map((e) => e.textContent)).toContain("body");
};

describe("ReviewFindingView", () => {
	// Catches: the Tasks thread hides the parsed finding, or renders its untrusted prompt as markup.
	it("tasks_thread_shows_finding_as_inert_text", () => {
		const { container } = render(() => (
			<TaskComment
				entry={{
					kind: "review_thread",
					value: {
						id: "t1",
						resolved: false,
						outdated: false,
						path: "a.tsx",
						line: 3,
						originalLine: 3,
						commitId: "head",
						comments: [
							{ ...comment, author: { id: "g", name: "greptile-apps", avatarUrl: null, bot: true }, url: null },
						],
						finding,
					},
				}}
			/>
		));
		assertInertFinding(container);
	});

	// Catches: the PR Files tab drops the backend finding, or renders its untrusted prompt as markup.
	it("pr_files_thread_shows_finding_as_inert_text", () => {
		const session = { busyThread: () => null } as unknown as ReviewSession;
		const { container } = render(() => (
			<ReviewThreadView
				session={session}
				thread={{
					id: "t1",
					path: "a.tsx",
					line: 3,
					originalLine: 3,
					diffSide: "RIGHT",
					isResolved: false,
					isOutdated: false,
					comments: {
						pageInfo: { hasNextPage: false, endCursor: null },
						nodes: [{ ...comment, author: { login: "greptile-apps", bot: true } }],
					},
					finding,
				}}
			/>
		));
		assertInertFinding(container);
	});
});
