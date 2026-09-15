import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";

// DiffViewer needs a real Canvas that jsdom/happy-dom don't provide (see
// DiffTab.test.tsx's note) — stubbed so this test exercises only StepCard's
// own wiring, not the diff renderer itself.
vi.mock("../../components/ui/DiffViewer", () => ({
	DiffViewer: (props: { diff: string }) => <div data-testid="diff-stub">{props.diff}</div>,
}));

const h = vi.hoisted(() => ({ invalidate: vi.fn() }));
vi.mock("../../components/DiffTab/useLineSelection", () => ({
	createLineSelection: () => ({
		selectedLines: () => new Set<number>(),
		selectedHunkIdx: () => null,
		handlers: { onMouseDown: () => {}, onMouseMove: () => {}, onMouseUp: () => {} },
		setContentRef: () => {},
		clear: () => {},
		invalidate: h.invalidate,
	}),
}));

import { StepCard } from "../../components/SessionDiffTab/StepCard";
import type { DiffViewMode } from "../../stores/ui";
import type { EditStep } from "../../types/sessionDiff";

const STEP: EditStep = {
	step_index: 0,
	tool_use_id: "toolu_abc",
	timestamp: null,
	kind: "edit",
	abs_path: "/repo/a.ts",
	rel_path: "a.ts",
	in_repo: true,
	patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
	additions: 1,
	deletions: 1,
	is_sidechain: false,
	agent_name: null,
	user_modified: false,
	replace_all: false,
};

describe("StepCard", () => {
	it("invalidates the line-selection row cache when mode changes (DiffViewer rebuilds its rows)", () => {
		const [mode, setMode] = createSignal<DiffViewMode>("unified");
		h.invalidate.mockClear();

		render(() => (
			<StepCard step={STEP} mode={mode()} onOpenAtLine={() => {}} onRevertStep={() => {}} onCopyStep={() => {}} />
		));
		// The effect runs once on mount too — clear that call so we only
		// assert on the mode-change-triggered one below.
		h.invalidate.mockClear();

		setMode("split");
		expect(h.invalidate).toHaveBeenCalledTimes(1);
	});
});
