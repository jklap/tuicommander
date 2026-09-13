import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { ProgressPanel } from "../../components/ProgressPanel";

const { longSummary, correct, openSource } = vi.hoisted(() => ({
	longSummary: "A validated outcome remains readable even when its recorded source session has already closed. ".repeat(
		8,
	),
	correct: vi.fn(),
	openSource: vi.fn(),
}));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		getPaths: () => ["/repo"],
		get: () => ({ displayName: "Example" }),
		getRevision: () => 0,
	},
}));

vi.mock("../../stores/progress", () => ({
	progressStore: {
		panelVisible: () => true,
		requestedProject: () => "/repo",
		requestedEvent: () => null,
		state: {
			projects: {
				"/repo": {
					status: {
						revision: 4,
						snapshotCursor: 4,
						readCursor: 0,
						unreadCount: 1,
						collectionEnabled: false,
						workstreams: [],
						projectBlockers: [],
					},
					events: [
						{
							id: "closed",
							sequence: 4,
							revision: 4,
							createdAtMs: 1,
							type: "blocked",
							summary: longSummary,
							workstream: "Safety",
							reporterName: "Worker",
							sessionId: "gone",
						},
					],
					error: "locked store",
				},
			},
		},
		refreshAll: vi.fn().mockResolvedValue(undefined),
		refreshProject: vi.fn(),
		loadMore: vi.fn(),
		markRead: vi.fn(),
		pause: vi.fn(),
		resume: vi.fn(),
		deleteEvents: vi.fn(),
		clear: vi.fn(),
		correct,
		isSourceLive: () => false,
		openSource,
		previewExport: vi.fn().mockResolvedValue({
			projectRoot: "/repo",
			path: "/repo/progress.md",
			snapshotId: "sha256:preview",
			snapshotRevision: 4,
			snapshotTimeMs: 1000,
			markdown: "# Project Progress: Example",
			fileExists: false,
			written: false,
		}),
		writeExport: vi.fn(),
		close: vi.fn(),
	},
}));

describe("ProgressPanel", () => {
	it("keeps long summaries, paused state, errors, and closed provenance readable", async () => {
		render(() => <ProgressPanel embedded />);
		expect(screen.getByText("Paused")).toBeTruthy();
		expect(screen.getByText((_, node) => node?.tagName === "P" && node.textContent === longSummary)).toBeTruthy();
		expect(screen.getByText(/locked store/)).toBeTruthy();
		await fireEvent.click(screen.getByText("Source"));
		const closed = screen.getByText("Session closed");
		expect((closed as HTMLButtonElement).disabled).toBe(true);
		await fireEvent.click(closed);
		expect(openSource).not.toHaveBeenCalled();
	});

	it("previews the selected project export before enabling the write", async () => {
		render(() => <ProgressPanel embedded />);
		await fireEvent.click(screen.getByText("Preview progress.md"));
		expect(screen.getByText("# Project Progress: Example")).toBeTruthy();
		expect(screen.getByText("Export progress.md")).toBeTruthy();
	});
});
