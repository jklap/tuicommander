import { describe, expect, it } from "vitest";
import { buildAnswersTurn } from "../answersTurn";
import { planCanvasMarkers, readMarkerPromptBoundary } from "../canvasTerminalMarkers";
import type { RowSnapshot } from "../suggestOverlay";
import claude from "./fixtures/claude-prompt-answer-1389.json";
import codex from "./fixtures/codex-answer-paragraphs-1872.json";

const snapshots = (texts: string[]): RowSnapshot[] => texts.map((text) => ({ text, isWrapped: false }));

describe("CanvasTerminal bounded viewport marker planner", () => {
	// Catches: a quoted marker becomes an answer when the submitted prompt is outside the 32-row lookback.
	it("retains recorded Claude prompt ownership above the viewport lookback", () => {
		const prompt = claude.rows.findIndex((text) => text.startsWith("❯"));
		const rows = snapshots(claude.rows.slice(prompt));
		rows.splice(1, 0, ...snapshots(Array.from({ length: 60 }, () => "  pasted context")));
		const viewport = rows.findIndex((row) => row.text.startsWith("  💬"));
		const answer = rows.findIndex((row) => row.text.startsWith("⏺ 💬"));
		expect(viewport).toBeGreaterThan(32);
		const plan = planCanvasMarkers(rows.length - viewport, (i) => rows[viewport + i] ?? null, [-viewport]);
		expect([...plan.answerMarkers]).toEqual([answer - viewport]);
		expect(plan.blocks.filter((block) => block.kind === "answer").map((block) => block.row)).toEqual(
			Array.from({ length: 7 }, (_, i) => answer - viewport + i),
		);
	});

	// Catches: prompt ownership swallows a genuine indented answer separated from the prompt by a blank row.
	it("keeps a blank-separated indented answer in the gutter and extracted turn", () => {
		const rows = snapshots(["⏺ 💬 First.", "", "  ❯ Next prompt", "", "  💬 Second."]);
		expect([...planCanvasMarkers(rows.length, (i) => rows[i] ?? null).answerMarkers]).toEqual([0, 4]);
		expect(buildAnswersTurn(rows.slice(2), true)).toEqual({ prompt: "  ❯ Next prompt", answers: ["💬 Second."] });
	});

	// Catches: recognizing only Claude's chevron lets Codex prompt text participate in answer classification.
	it("keeps the recorded Codex prompt separate from its marked answer paragraphs", () => {
		const rows = snapshots(codex.rows);
		const prompt = codex.rows.findIndex((text) => text.startsWith("›"));
		const plan = planCanvasMarkers(rows.length, (i) => rows[i] ?? null, [prompt]);
		expect([...plan.answerMarkers]).toEqual([18, 20, 22]);
		expect(buildAnswersTurn(rows.slice(prompt, 25), false).answers).toEqual([
			"💬 The answer highlight runs continuously across the full response.\n\n💬 Blank paragraph spacing stays preserved.\n\n💬 Raw copy text remains untouched.",
		]);
	});
	// Catches: a cold paint cache fails open on a quoted marker instead of reading the known prompt prefix.
	it("reads missing prompt ownership before classifying a cold viewport", async () => {
		const prompt = claude.rows.findIndex((text) => text.startsWith("❯"));
		const rows = snapshots(claude.rows.slice(prompt));
		rows.splice(1, 0, ...snapshots(Array.from({ length: 60 }, () => "  pasted context")));
		const viewport = rows.findIndex((row) => row.text.startsWith("  💬"));
		const getRow = (i: number) => (i < -32 ? null : (rows[viewport + i] ?? null));
		const pending = planCanvasMarkers(rows.length - viewport, getRow, [-viewport]);
		expect(pending.pendingPrompt).toBe(-viewport);
		expect([...pending.answerMarkers]).toEqual([]);
		const boundary = await readMarkerPromptBoundary(0, rows.length, async (from, to) => rows.slice(from, to));
		expect(boundary).toEqual({ end: viewport + 8, resolved: true });
		const ready = planCanvasMarkers(rows.length - viewport, getRow, [-viewport], {
			end: boundary!.end - viewport,
			resolved: boundary!.resolved,
		});
		expect([...ready.answerMarkers]).toEqual([8]);
	});

	// Catches: missing the old output bullet suppresses genuine marked continuation paragraphs in a later viewport.
	it("releases an offscreen prompt when output started before the lookback", async () => {
		const rows = snapshots([
			"❯ Question",
			"⏺ First output",
			...Array.from({ length: 80 }, () => "  continuation"),
			"  💬 Answer",
		]);
		const viewport = rows.length - 1;
		const boundary = await readMarkerPromptBoundary(0, rows.length, async (from, to) => rows.slice(from, to));
		const plan = planCanvasMarkers(1, (i) => (i < -32 ? null : (rows[viewport + i] ?? null)), [-viewport], {
			end: boundary!.end - viewport,
			resolved: boundary!.resolved,
		});
		expect([...plan.answerMarkers]).toEqual([0]);
	});

	// Catches: a chunk edge turns a soft-wrapped quoted marker into a new output marker.
	it("preserves wrapped prompt markers across ownership-read chunks", async () => {
		const rows = snapshots([
			"› Question",
			...Array.from({ length: 254 }, () => "  context"),
			"",
			"  💬 quoted",
			"• 💬 Answer",
		]);
		rows[256].isWrapped = true;
		const reads: [number, number][] = [];
		expect(
			await readMarkerPromptBoundary(0, rows.length, async (from, to) => {
				reads.push([from, to]);
				return rows.slice(from, to);
			}),
		).toEqual({ end: 257, resolved: true });
		expect(reads).toEqual([
			[0, 256],
			[255, 258],
		]);
	});

	// Catches: rescanning the accumulated prefix makes long-prompt classification quadratic.
	it("classifies each ownership-read chunk once instead of rescanning the prefix", async () => {
		let accesses = 0;
		const count = 8192;
		const rows = Array.from({ length: count }, () => ({
			get text() {
				accesses++;
				return "  pasted context";
			},
			isWrapped: false,
		}));
		expect(await readMarkerPromptBoundary(0, count, async (from, to) => rows.slice(from, to))).toEqual({
			end: count,
			resolved: false,
		});
		// Each nonwrapped row needs at most the output and answer-marker checks.
		expect(accesses).toBeLessThanOrEqual(count * 2);
	});

	// Catches: incremental chunk scans forget the preceding blank row at the boundary.
	it("recognizes blank-separated output at the first row of a later chunk", async () => {
		const rows = snapshots(["❯ Question", ...Array.from({ length: 254 }, () => "  context"), "", "  💬 Answer"]);
		expect(await readMarkerPromptBoundary(0, rows.length, async (from, to) => rows.slice(from, to))).toEqual({
			end: 256,
			resolved: true,
		});
	});

	// Catches: a failed ownership read is promoted to an empty prompt and exposes quoted markers.
	it("does not classify a failed prompt read as an output boundary", async () => {
		expect(await readMarkerPromptBoundary(0, 100, async () => null)).toBeNull();
	});

	// Catches: Codex's different prompt glyph exposes a quoted hard-line marker as an answer.
	it.each(["❯", "›"])("keeps an uninterrupted quoted marker inside a %s prompt", (glyph) => {
		const rows = snapshots([`${glyph} Question`, "  quoted preface", "  💬 Quoted example", "• 💬 Answer"]);
		expect([...planCanvasMarkers(rows.length, (i) => rows[i] ?? null).answerMarkers]).toEqual([3]);
		expect(buildAnswersTurn(rows, false).answers).toEqual(["💬 Answer"]);
	});
});
