import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, createMemo, For, type JSX, Show } from "solid-js";
import type { DiffViewMode } from "../../stores/ui";
import type { EditStep, FileReview } from "../../types/sessionDiff";
import fl from "../shared/diffFileList.module.css";
import { DiffViewer } from "../ui/DiffViewer";
import { rowKey, type SessionRow } from "./buildRows";
import s from "./SessionDiffTab.module.css";
import { SessionFileHeader } from "./SessionFileHeader";
import { StepCard } from "./StepCard";

interface SessionRowContentProps {
	row: () => SessionRow;
	mode: DiffViewMode;
	onOpenFile: (path: string) => void;
	onOpenAtLine: (step: EditStep, line: number) => void;
	onRevertStep: (step: EditStep) => void;
	onRevertFile: (group: FileReview) => void;
	onCopyStep: (step: EditStep) => void;
	onCopyFile: (group: FileReview) => void;
	onToggleExpanded: (absPath: string) => void;
	onToggleStepsOpen: (absPath: string) => void;
}

/**
 * Renders a single row's content by reading `props.row()` through memos, so a
 * data update under the SAME key (a live refresh, or a toggled `expanded`/
 * `stepsOpen` flag) is reflected in place instead of staying frozen at
 * whatever the row looked like when this slot was first mounted. The parent
 * `<Show when={rowKey(row())} keyed>` handles the OTHER case — a DIFFERENT
 * row landing in this virtual slot — by remounting this component fresh
 * whenever the key itself changes.
 */
const SessionRowContent: Component<SessionRowContentProps> = (props) => {
	const fileRow = createMemo(() => {
		const row = props.row();
		return row.kind === "file" ? row : null;
	});
	const stepRow = createMemo(() => {
		const row = props.row();
		return row.kind === "step" ? row : null;
	});

	return (
		<>
			<Show when={fileRow()}>
				{(fr) => (
					<div class={fl.fileSection}>
						<SessionFileHeader
							group={fr().group}
							stepCount={fr().steps.length}
							expanded={fr().expanded}
							stepsOpen={fr().stepsOpen}
							onToggleExpanded={() => props.onToggleExpanded(fr().group.abs_path)}
							onToggleStepsOpen={() => props.onToggleStepsOpen(fr().group.abs_path)}
							onOpenFile={() => props.onOpenFile(fr().group.abs_path)}
							onRevertFile={() => props.onRevertFile(fr().group)}
							onCopyFile={() => props.onCopyFile(fr().group)}
						/>
						<Show when={fr().expanded}>
							<div class={fl.fileDiff}>
								<DiffViewer
									diff={fr().group.cumulative_patch}
									mode={props.mode}
									emptyMessage={
										fr().group.base_source === "unknown"
											? "Can't compute a cumulative diff for this file"
											: "No net change"
									}
								/>
							</div>
							<Show when={fr().stepsOpen}>
								<div class={s.nestedSteps}>
									<For each={fr().steps}>
										{(step) => (
											<StepCard
												step={step}
												mode={props.mode}
												onOpenAtLine={props.onOpenAtLine}
												onRevertStep={props.onRevertStep}
												onCopyStep={props.onCopyStep}
											/>
										)}
									</For>
								</div>
							</Show>
						</Show>
					</div>
				)}
			</Show>
			<Show when={stepRow()}>
				{(sr) => (
					<StepCard
						step={sr().step}
						mode={props.mode}
						showFilePath
						onOpenAtLine={props.onOpenAtLine}
						onRevertStep={props.onRevertStep}
						onCopyStep={props.onCopyStep}
					/>
				)}
			</Show>
		</>
	);
};

export interface SessionDiffListProps {
	rows: SessionRow[];
	mode: DiffViewMode;
	onOpenFile: (path: string) => void;
	onOpenAtLine: (step: EditStep, line: number) => void;
	onRevertStep: (step: EditStep) => void;
	onRevertFile: (group: FileReview) => void;
	onCopyStep: (step: EditStep) => void;
	onCopyFile: (group: FileReview) => void;
	onToggleExpanded: (absPath: string) => void;
	onToggleStepsOpen: (absPath: string) => void;
	scrollRef?: (el: HTMLElement) => void;
	header?: JSX.Element;
}

/**
 * Virtualized list serving BOTH the grouped-by-file (default) and flat
 * chronological views — `buildRows()` already decided which rows exist, this
 * just renders them. Copied by value (not shared) from `DiffFileList`: the
 * `createVirtualizer` config, `top`-not-`transform` positioning (required for
 * the file header's `position: sticky` to keep working), and dynamic sizing
 * via `measureElement` — session rows have a different item shape (a file row
 * can also show its nested step cards) that doesn't fit `DiffFileList`'s
 * simpler one-collapse-state-per-item model.
 *
 * Each virtual slot's content is keyed by `rowKey()` (`<Show ... keyed>`), NOT
 * read as a plain captured value — `@tanstack/solid-virtual` reconciles
 * virtual items by POSITION (`reconcile(..., {key:"index"})`), so a plain
 * `const row = props.rows[vi.index]` read once at slot-creation time would
 * never see a later update (a mode switch, a session switch, a live refresh,
 * or a toggled `expanded` flag) for a slot that's already mounted. Keying the
 * `<Show>` on `rowKey(row())` gets both cases right: the SAME key with new
 * data updates `SessionRowContent` in place (preserving scroll position, open
 * comment boxes, line selection), while a DIFFERENT key remounts it fresh.
 *
 * DEFERRED (same as DiffFileList): Cmd+F via DomSearchEngine only matches
 * *mounted* rows — an off-screen file/step isn't in the DOM.
 */
export const SessionDiffList: Component<SessionDiffListProps> = (props) => {
	let scrollEl: HTMLDivElement | undefined;

	const virtualizer = createVirtualizer({
		get count() {
			return props.rows.length;
		},
		getScrollElement: () => scrollEl ?? null,
		estimateSize: () => 320,
		overscan: 3,
		getItemKey: (i) => {
			const row = props.rows[i];
			return row ? rowKey(row) : i;
		},
	});

	return (
		<div
			class={fl.container}
			ref={(el) => {
				scrollEl = el;
				props.scrollRef?.(el);
			}}
		>
			{props.header}
			<div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative", width: "100%" }}>
				<For each={virtualizer.getVirtualItems()}>
					{(vi) => {
						const row = () => props.rows[vi.index];
						return (
							<div
								data-index={vi.index}
								ref={(el) => virtualizer.measureElement(el)}
								style={{ position: "absolute", top: `${vi.start}px`, left: "0", width: "100%" }}
							>
								<Show when={rowKey(row())} keyed>
									{() => (
										<SessionRowContent
											row={row}
											mode={props.mode}
											onOpenFile={props.onOpenFile}
											onOpenAtLine={props.onOpenAtLine}
											onRevertStep={props.onRevertStep}
											onRevertFile={props.onRevertFile}
											onCopyStep={props.onCopyStep}
											onCopyFile={props.onCopyFile}
											onToggleExpanded={props.onToggleExpanded}
											onToggleStepsOpen={props.onToggleStepsOpen}
										/>
									)}
								</Show>
							</div>
						);
					}}
				</For>
			</div>
		</div>
	);
};
