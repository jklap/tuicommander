import { render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { TurnPicker } from "../../components/SessionDiffTab/TurnPicker";
import type { EditStep, TurnSummary } from "../../types/sessionDiff";

function turn(overrides: Partial<TurnSummary> = {}): TurnSummary {
	return {
		turn_index: 0,
		started_at: null,
		prompt_preview: "Fix the bug",
		step_indices: [0],
		additions: 1,
		deletions: 1,
		files: ["a.ts"],
		...overrides,
	};
}

function step(overrides: Partial<EditStep> = {}): EditStep {
	return {
		step_index: 0,
		tool_use_id: "toolu_1",
		timestamp: null,
		kind: "edit",
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		patch: "@@ -1,1 +1,1 @@\n-old\n+new\n",
		additions: 1,
		deletions: 1,
		is_sidechain: false,
		agent_name: null,
		user_modified: false,
		replace_all: false,
		turn_index: 0,
		turn_started_at: null,
		prompt_preview: null,
		agent_id: null,
		agent_display_name: null,
		...overrides,
	};
}

describe("TurnPicker", () => {
	it("the trigger shows the turn count and is disabled with zero turns", () => {
		const { getByText } = render(() => <TurnPicker turns={[]} steps={[]} onSelect={() => {}} />);
		const btn = getByText("Turns (0)") as HTMLButtonElement;
		expect(btn.disabled).toBe(true);
	});

	it("clicking the trigger opens a panel listing each turn's time, size, and files", () => {
		const turns = [
			turn({ turn_index: 0, additions: 3, deletions: 1, files: ["a.ts", "b.ts"] }),
			turn({ turn_index: 1, step_indices: [5], additions: 0, deletions: 2, files: ["c.ts"] }),
		];
		const { getByText, getByTestId } = render(() => <TurnPicker turns={turns} steps={[]} onSelect={() => {}} />);
		getByText("Turns (2)").click();
		const panel = getByTestId("turn-picker-panel");
		expect(panel.textContent).toContain("+3");
		expect(panel.textContent).toContain("-1");
		expect(panel.textContent).toContain("a.ts, b.ts");
		expect(panel.textContent).toContain("+0");
		expect(panel.textContent).toContain("-2");
		expect(panel.textContent).toContain("c.ts");
	});

	it("truncates a long file list with an overflow count", () => {
		const turns = [turn({ files: ["a.ts", "b.ts", "c.ts", "d.ts"] })];
		const { getByText, getByTestId } = render(() => <TurnPicker turns={turns} steps={[]} onSelect={() => {}} />);
		getByText("Turns (1)").click();
		expect(getByTestId("turn-picker-panel").textContent).toContain("a.ts, b.ts +2 more");
	});

	it("clicking a turn calls onSelect with that turn's first step_index and closes the panel", () => {
		const onSelect = vi.fn();
		const turns = [turn({ step_indices: [7, 8] })];
		const { getByText, getByTestId, queryByTestId } = render(() => (
			<TurnPicker turns={turns} steps={[]} onSelect={onSelect} />
		));
		getByText("Turns (1)").click();
		getByTestId("turn-picker-panel")
			.querySelector("button")
			?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
		expect(onSelect).toHaveBeenCalledWith(7);
		expect(queryByTestId("turn-picker-panel")).toBeNull();
	});

	it("the tooltip combines the prompt preview with the first touched step's +/- lines", () => {
		const turns = [turn({ step_indices: [0], prompt_preview: "Fix the bug" })];
		const steps = [step({ step_index: 0, patch: "@@ -1,2 +1,2 @@\n-old line\n+new line\n context\n" })];
		const { getByText, getByTestId } = render(() => <TurnPicker turns={turns} steps={steps} onSelect={() => {}} />);
		getByText("Turns (1)").click();
		const item = getByTestId("turn-picker-panel").querySelector("button") as HTMLButtonElement;
		expect(item.title).toContain("Fix the bug");
		expect(item.title).toContain("-old line");
		expect(item.title).toContain("+new line");
	});

	it("falls back to a placeholder tooltip when there's no prompt preview or resolvable step", () => {
		const turns = [turn({ prompt_preview: null, step_indices: [99] })];
		const { getByText, getByTestId } = render(() => <TurnPicker turns={turns} steps={[]} onSelect={() => {}} />);
		getByText("Turns (1)").click();
		const item = getByTestId("turn-picker-panel").querySelector("button") as HTMLButtonElement;
		expect(item.title).toBe("(no prompt text)");
	});
});
