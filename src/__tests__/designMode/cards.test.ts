import { describe, expect, it } from "vitest";
// Extension modules run directly in Chromium; no build step changes their state model.
// @ts-expect-error plain MV3 module
import { addPick, deletePick, draftForTab, editPick } from "../../../browser-extension/design-mode/cards.js";

describe("Design Mode drafts", () => {
	// Catches: editing/deleting one pick overwriting another tab's feedback.
	it("preserves comments and isolates picks by tab", () => {
		let draft = addPick({ picks: [] }, { id: "one", raw: { selector: "#first" } });
		draft = addPick(draft, { id: "two", raw: { selector: "#second" } });
		draft = editPick(draft, "one", "Use more spacing");
		expect(draft.picks.map((pick: { comment: string }) => pick.comment)).toEqual(["Use more spacing", ""]);
		expect(deletePick(draft, "two").picks).toEqual([
			expect.objectContaining({ id: "one", comment: "Use more spacing" }),
		]);
		expect(draftForTab({ "42": draft }, 43).picks).toEqual([]);
	});
	// Catches: unbounded collection exhausting storage and losing all drafts.
	it("refuses the thirty-third pick without losing existing comments", () => {
		let draft = { picks: [] };
		for (let n = 0; n < 32; n++) draft = addPick(draft, { id: String(n), raw: {} });
		expect(() => addPick(draft, { id: "overflow" })).toThrow("32 maximum");
		expect(draft.picks).toHaveLength(32);
	});
});

describe("Design Mode agent selection", () => {
	// Catches: ignoring the terminal that started Design Mode and selecting a different agent.
	it("selects the launching session, preserves a switch and reports an exited selection", async () => {
		// @ts-expect-error plain MV3 module
		const { recipient } = await import("../../../browser-extension/design-mode/cards.js");
		const agents = [
			{ sessionId: "Y", label: "Other" },
			{ sessionId: "X", label: "Launching" },
		];
		expect(recipient({ picks: [] }, agents, "X").selectedSessionId).toBe("X");
		expect(
			recipient({ picks: [], selectionInitialized: true, selectedSessionId: "Y" }, agents, "X").selectedSessionId,
		).toBe("Y");
		const exited = recipient({ picks: [], selectionInitialized: true, selectedSessionId: "X" }, [agents[0]], "X");
		expect(exited.selectedSessionId).toBe("");
		expect(exited.agentNotice).toContain("exited");
		expect(recipient({ picks: [] }, agents).selectedSessionId).toBe("Y");
	});
});

describe("Shared Markdown and Design Mode recipients", () => {
	// Catches: offering shells or agents belonging to another repository.
	it("offers only same-repository live agent sessions in existing order", async () => {
		const { reviewAgents } = await import("../../../browser-extension/design-mode/reviewAgents.js");
		expect(
			reviewAgents(
				[
					{ sessionId: "shell", repoPath: "/repo", name: "Shell" },
					{ sessionId: "other", repoPath: "/other", agentType: "codex", name: "Other" },
					{ sessionId: "first", repoPath: "/repo", agentType: "claude", name: "First" },
					{ sessionId: "second", repoPath: "/repo", agentType: "codex", name: "Second" },
				],
				"/repo",
			),
		).toEqual([
			{ sessionId: "first", label: "First" },
			{ sessionId: "second", label: "Second" },
		]);
	});
});

describe("Design feedback delivery", () => {
	// Catches: failed or lost delivery replies discarding cards or retrying with another key.
	it("retains failed drafts, retries the same batch and shows queued acceptance", async () => {
		// @ts-expect-error plain MV3 module
		const { deliverDraft } = await import("../../../browser-extension/design-mode/cards.js");
		let stored = {
			selectedSessionId: "X",
			picks: [
				{ id: "one", comment: "More space", raw: {} },
				{ id: "two", comment: "", raw: {} },
			],
		};
		const save = async (value: typeof stored) => {
			stored = value;
		};
		await expect(
			deliverDraft(
				stored,
				async () => {
					throw new Error("Disconnected");
				},
				save,
				"batch-1",
			),
		).rejects.toThrow("Disconnected");
		expect(stored.picks).toHaveLength(2);
		const batches: unknown[] = [];
		const result = await deliverDraft(
			stored,
			async (batch: unknown) => {
				batches.push(batch);
				return { accepted: true, typed: false, queued: 1 };
			},
			save,
			"batch-2",
		);
		expect(batches).toEqual([expect.objectContaining({ idempotencyKey: "batch-1", sessionId: "X" })]);
		expect(result.typed).toBe(false);
		expect(stored.picks).toEqual([expect.objectContaining({ id: "two" })]);
	});
});
