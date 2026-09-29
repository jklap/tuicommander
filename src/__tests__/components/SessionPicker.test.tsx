import { render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { SessionPicker } from "../../components/SessionDiffTab/SessionPicker";
import type { SessionSummary } from "../../types/sessionDiff";

function summary(overrides: Partial<SessionSummary>): SessionSummary {
	return {
		session_id: "sess-1",
		transcript_path: "/x.jsonl",
		cwd: "/repo",
		git_branch: "main",
		started_at: new Date(Date.now() - 5 * 60_000).toISOString(),
		ended_at: null,
		title: "My session",
		last_prompt: null,
		size_bytes: 100,
		edit_count: 3,
		file_count: 2,
		has_subagents: false,
		...overrides,
	};
}

describe("SessionPicker", () => {
	it("shows the selected session's title, relative time, and counts in the trigger", () => {
		const sess = summary({});
		const { getByTitle } = render(() => (
			<SessionPicker
				sessions={[sess]}
				selectedId="sess-1"
				liveId={null}
				loading={false}
				onSelect={vi.fn()}
				onRefresh={vi.fn()}
			/>
		));
		const trigger = getByTitle("Choose which Claude Code session to review");
		expect(trigger.textContent).toContain("My session");
		expect(trigger.textContent).toContain("2 files, 3 edits");
	});

	it("shows an empty-list message when there are no sessions", () => {
		const { getByTitle } = render(() => (
			<SessionPicker
				sessions={[]}
				selectedId={null}
				liveId={null}
				loading={false}
				onSelect={vi.fn()}
				onRefresh={vi.fn()}
			/>
		));
		expect(getByTitle("Choose which Claude Code session to review").textContent).toContain("No Claude sessions found");
	});

	it("shows a loading message while sessions are being fetched", () => {
		const { getByTitle } = render(() => (
			<SessionPicker
				sessions={[]}
				selectedId={null}
				liveId={null}
				loading={true}
				onSelect={vi.fn()}
				onRefresh={vi.fn()}
			/>
		));
		expect(getByTitle("Choose which Claude Code session to review").textContent).toContain("Loading sessions");
	});

	it("selecting an item calls onSelect with that session's id", () => {
		const sessA = summary({ session_id: "sess-a", title: "Session A" });
		const sessB = summary({ session_id: "sess-b", title: "Session B" });
		const onSelect = vi.fn();
		const { getByTitle, getByText } = render(() => (
			<SessionPicker
				sessions={[sessA, sessB]}
				selectedId="sess-a"
				liveId={null}
				loading={false}
				onSelect={onSelect}
				onRefresh={vi.fn()}
			/>
		));
		getByTitle("Choose which Claude Code session to review").click();
		getByText(/Session B/).click();
		expect(onSelect).toHaveBeenCalledWith("sess-b");
	});

	it("marks the live session's item with a leading dot", () => {
		const sess = summary({ session_id: "sess-live" });
		const { getByTitle, getByText } = render(() => (
			<SessionPicker
				sessions={[sess]}
				selectedId="sess-live"
				liveId="sess-live"
				loading={false}
				onSelect={vi.fn()}
				onRefresh={vi.fn()}
			/>
		));
		getByTitle("Choose which Claude Code session to review").click();
		expect(getByText(/●.*My session/)).toBeTruthy();
	});

	it("the refresh button calls onRefresh", () => {
		const onRefresh = vi.fn();
		const { getByTitle } = render(() => (
			<SessionPicker
				sessions={[]}
				selectedId={null}
				liveId={null}
				loading={false}
				onSelect={vi.fn()}
				onRefresh={onRefresh}
			/>
		));
		getByTitle("Refresh session list").click();
		expect(onRefresh).toHaveBeenCalled();
	});
});
