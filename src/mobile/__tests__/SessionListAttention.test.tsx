// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QuestionBanner } from "../components/QuestionBanner";
import { StatusBadge } from "../components/StatusBadge";
import { TopBar } from "../components/TopBar";
import type { SessionInfo } from "../useSessions";

afterEach(cleanup);

const waiting = {
	session_id: "review",
	display_name: "Wiz-Pr",
	cwd: "/work/tuicommander",
	worktree_path: null,
	worktree_branch: null,
	state: { awaiting_input: true, agent_type: "claude", question_text: "Approve?" },
} as SessionInfo;

describe("mobile session attention", () => {
	it("keeps the exact casing of session names across list, detail, and banner", () => {
		for (const relativePath of [
			"../components/SessionCard.module.css",
			"../screens/SessionDetailScreen.module.css",
			"../components/QuestionBanner.module.css",
		]) {
			const css = readFileSync(resolve(__dirname, relativePath), "utf-8");
			expect(css, relativePath).not.toMatch(/text-transform:\s*capitalize/);
		}
	});
	it("names the waiting session and repository in its question banner", () => {
		const view = render(() => <QuestionBanner sessions={[waiting]} onNavigate={() => {}} />);
		const banner = view.getByRole("button", { name: /Wiz-Pr/ });
		expect(banner.textContent).toContain("tuicommander");
		expect(banner.textContent).toContain("Approve?");
	});

	it("opens a waiting session from the counter", async () => {
		const onNavigate = vi.fn();
		const view = render(() => (
			<TopBar notificationCount={1} isConnected onNotificationsClick={() => onNavigate(waiting.session_id)} />
		));
		await fireEvent.click(view.getByRole("button", { name: "Open 1 waiting session" }));
		expect(onNavigate).toHaveBeenCalledWith("review");
	});

	it("calls the active state Working", () => {
		const view = render(() => <StatusBadge status="busy" />);
		expect(view.getByText("Working")).toBeTruthy();
	});
});
