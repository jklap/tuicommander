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
	turn_index: 0,
	turn_started_at: null,
	prompt_preview: null,
	agent_id: null,
	agent_display_name: null,
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

	describe("subagent badge / jump-to-agent", () => {
		const SUBAGENT_STEP: EditStep = {
			...STEP,
			is_sidechain: true,
			agent_name: "a19570f6f",
			agent_display_name: "scan-vsstack-a",
		};

		it("renders a plain span (no button) when onJumpToAgent is not supplied", () => {
			const { container, getByText } = render(() => (
				<StepCard
					step={SUBAGENT_STEP}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
				/>
			));
			expect(getByText("subagent: scan-vsstack-a")).toBeTruthy();
			expect(container.querySelector("button.badgeLink, button[class*='badgeLink']")).toBeNull();
		});

		it("renders the badge as a clickable button when onJumpToAgent is supplied, and clicking it calls the handler", () => {
			const onJumpToAgent = vi.fn();
			const { getByRole } = render(() => (
				<StepCard
					step={SUBAGENT_STEP}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
					onJumpToAgent={onJumpToAgent}
				/>
			));
			const btn = getByRole("button", { name: "subagent: scan-vsstack-a" });
			btn.click();
			expect(onJumpToAgent).toHaveBeenCalledOnce();
		});

		it("falls back to agent_name when agent_display_name is absent", () => {
			const { getByText } = render(() => (
				<StepCard
					step={{ ...SUBAGENT_STEP, agent_display_name: null }}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
				/>
			));
			expect(getByText("subagent: a19570f6f")).toBeTruthy();
		});

		it("shows a bare 'subagent' label with no name when neither is present", () => {
			const { getByText } = render(() => (
				<StepCard
					step={{ ...SUBAGENT_STEP, agent_name: null, agent_display_name: null }}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
				/>
			));
			expect(getByText("subagent")).toBeTruthy();
		});
	});

	describe("collapse caret", () => {
		it("clicking anywhere on the header toggles collapse", () => {
			const onToggleCollapsed = vi.fn();
			const { container } = render(() => (
				<StepCard
					step={STEP}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
					onToggleCollapsed={onToggleCollapsed}
				/>
			));
			(container.querySelector('[role="button"]') as HTMLElement).click();
			expect(onToggleCollapsed).toHaveBeenCalledOnce();
		});

		it("action buttons (copy/revert/open-at-line) don't also trigger collapse", () => {
			const onToggleCollapsed = vi.fn();
			const onCopyStep = vi.fn();
			const { getByTitle } = render(() => (
				<StepCard
					step={STEP}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={onCopyStep}
					onToggleCollapsed={onToggleCollapsed}
				/>
			));
			getByTitle("Copy this step's diff").click();
			expect(onCopyStep).toHaveBeenCalledOnce();
			expect(onToggleCollapsed).not.toHaveBeenCalled();
		});

		it("hides the diff body when collapsed", () => {
			const { queryByTestId } = render(() => (
				<StepCard
					step={STEP}
					mode="unified"
					collapsed
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
				/>
			));
			expect(queryByTestId("diff-stub")).toBeNull();
		});

		it("shows the diff body when not collapsed (default)", () => {
			const { queryByTestId } = render(() => (
				<StepCard step={STEP} mode="unified" onOpenAtLine={() => {}} onRevertStep={() => {}} onCopyStep={() => {}} />
			));
			expect(queryByTestId("diff-stub")).not.toBeNull();
		});
	});

	describe("same-file jump buttons (^/v)", () => {
		it("renders neither button when showFilePath is false, even with handlers supplied", () => {
			const { queryByTitle } = render(() => (
				<StepCard
					step={STEP}
					mode="unified"
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
					onJumpPrev={() => {}}
					onJumpNext={() => {}}
				/>
			));
			expect(queryByTitle("Jump to the earlier change to this file")).toBeNull();
			expect(queryByTitle("Jump to the later change to this file")).toBeNull();
		});

		it("hides a button when its handler is undefined (no earlier/later touch)", () => {
			const { queryByTitle, getByTitle } = render(() => (
				<StepCard
					step={STEP}
					mode="unified"
					showFilePath
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
					onJumpNext={() => {}}
				/>
			));
			expect(queryByTitle("Jump to the earlier change to this file")).toBeNull();
			expect(getByTitle("Jump to the later change to this file")).toBeTruthy();
		});

		it("clicking a jump button calls its handler and does not toggle collapse", () => {
			const onJumpPrev = vi.fn();
			const onToggleCollapsed = vi.fn();
			const { getByTitle } = render(() => (
				<StepCard
					step={STEP}
					mode="unified"
					showFilePath
					onOpenAtLine={() => {}}
					onRevertStep={() => {}}
					onCopyStep={() => {}}
					onJumpPrev={onJumpPrev}
					onToggleCollapsed={onToggleCollapsed}
				/>
			));
			getByTitle("Jump to the earlier change to this file").click();
			expect(onJumpPrev).toHaveBeenCalledOnce();
			expect(onToggleCollapsed).not.toHaveBeenCalled();
		});
	});
});
