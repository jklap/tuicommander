import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, createEffect, createMemo, createSignal, For, type JSX, Show } from "solid-js";
import type { DiffViewMode } from "../../stores/ui";
import type { EditStep, FileReview } from "../../types/sessionDiff";
import { cx } from "../../utils";
import fl from "../shared/diffFileList.module.css";
import type { DiffListNavHandle } from "../shared/diffListNav";
import { DiffViewer } from "../ui/DiffViewer";
import { rowKey, type SessionRow, type StepRowData } from "./buildRows";
import s from "./SessionDiffTab.module.css";
import { SessionFileHeader } from "./SessionFileHeader";
import { StepCard } from "./StepCard";

interface SessionRowContentProps {
	row: () => SessionRow;
	mode: DiffViewMode;
	wrap?: boolean;
	maxLines?: number;
	/** This row's file just changed via a live update while visible — apply a
	 *  brief flash-and-fade so the user can eyeball what changed. */
	flash?: boolean;
	onOpenFile: (path: string) => void;
	onOpenAtLine: (step: EditStep, line: number) => void;
	onRevertStep: (step: EditStep) => void;
	onRevertFile: (group: FileReview) => void;
	onCopyStep: (step: EditStep) => void;
	onCopyFile: (group: FileReview) => void;
	onToggleExpanded: (absPath: string) => void;
	onToggleStepsOpen: (absPath: string) => void;
	onToggleStepCollapsed: (toolUseId: string) => void;
	onJumpToStep: (stepIndex: number) => void;
	onJumpToAgent?: () => void;
}

/** A step's `^`/`v` targets resolve to `onJumpToStep(stepIndex)` — the entry
 *  already carries which step_index, if any, to jump to on each side. */
function stepJumpHandlers(entry: StepRowData, onJumpToStep: (stepIndex: number) => void) {
	const prev = entry.prevSameFileStepIndex;
	const next = entry.nextSameFileStepIndex;
	return {
		onJumpPrev: prev !== null ? () => onJumpToStep(prev) : undefined,
		onJumpNext: next !== null ? () => onJumpToStep(next) : undefined,
	};
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
					<div class={cx(fl.fileSection, props.flash && fl.flash)}>
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
									wrap={props.wrap}
									maxLines={props.maxLines}
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
										{(entry) => (
											<StepCard
												step={entry.step}
												mode={props.mode}
												wrap={props.wrap}
												maxLines={props.maxLines}
												collapsed={entry.collapsed}
												onToggleCollapsed={() => props.onToggleStepCollapsed(entry.step.tool_use_id)}
												onOpenAtLine={props.onOpenAtLine}
												onRevertStep={props.onRevertStep}
												onCopyStep={props.onCopyStep}
												onJumpToAgent={props.onJumpToAgent}
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
						wrap={props.wrap}
						maxLines={props.maxLines}
						showFilePath
						collapsed={sr().collapsed}
						onToggleCollapsed={() => props.onToggleStepCollapsed(sr().step.tool_use_id)}
						{...stepJumpHandlers(sr(), props.onJumpToStep)}
						onOpenAtLine={props.onOpenAtLine}
						onRevertStep={props.onRevertStep}
						onCopyStep={props.onCopyStep}
						onJumpToAgent={props.onJumpToAgent}
					/>
				)}
			</Show>
		</>
	);
};

export interface SessionDiffListProps {
	rows: SessionRow[];
	mode: DiffViewMode;
	wrap?: boolean;
	maxLines?: number;
	/** `abs_path`s of file rows to flash-highlight right now — a live update
	 *  just applied to a row that was already visible. */
	flashKeys?: ReadonlySet<string>;
	onOpenFile: (path: string) => void;
	onOpenAtLine: (step: EditStep, line: number) => void;
	onRevertStep: (step: EditStep) => void;
	onRevertFile: (group: FileReview) => void;
	onCopyStep: (step: EditStep) => void;
	onCopyFile: (group: FileReview) => void;
	onToggleExpanded: (absPath: string) => void;
	onToggleStepsOpen: (absPath: string) => void;
	onToggleStepCollapsed: (toolUseId: string) => void;
	onJumpToStep: (stepIndex: number) => void;
	onJumpToAgent?: () => void;
	scrollRef?: (el: HTMLElement) => void;
	header?: JSX.Element;
	ref?: (handle: DiffListNavHandle) => void;
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

	// `getVirtualItems()` is already Solid's own reactive read for this
	// virtualizer (used below inside `<For>`) — tracking it here too keeps
	// `currentIndex`/`visibleIndices` live as the user scrolls, without
	// reimplementing any of the virtualizer's own range math.
	const [currentIndex, setCurrentIndex] = createSignal(0);
	const [visibleIndices, setVisibleIndices] = createSignal<ReadonlySet<number>>(new Set());
	createEffect(() => {
		const items = virtualizer.getVirtualItems();
		if (items.length > 0) setCurrentIndex(items[0].index);
		setVisibleIndices(new Set(items.map((i) => i.index)));
	});
	props.ref?.({
		scrollToIndex: (index, opts) => virtualizer.scrollToIndex(index, { align: opts?.align ?? "auto" }),
		currentIndex,
		rowCount: () => props.rows.length,
		visibleIndices,
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
						const isFlashing = () => {
							const r = row();
							return r.kind === "file" && (props.flashKeys?.has(r.group.abs_path) ?? false);
						};
						return (
							<div
								data-index={vi.index}
								ref={(el) => virtualizer.measureElement(el)}
								style={{ position: "absolute", top: `${vi.start}px`, left: "0", width: "100%" }}
							>
								<Show when={rowKey(row())} keyed>
									{(_key) => (
										<SessionRowContent
											row={row}
											mode={props.mode}
											wrap={props.wrap}
											maxLines={props.maxLines}
											flash={isFlashing()}
											onOpenFile={props.onOpenFile}
											onOpenAtLine={props.onOpenAtLine}
											onRevertStep={props.onRevertStep}
											onRevertFile={props.onRevertFile}
											onCopyStep={props.onCopyStep}
											onCopyFile={props.onCopyFile}
											onToggleExpanded={props.onToggleExpanded}
											onToggleStepsOpen={props.onToggleStepsOpen}
											onToggleStepCollapsed={props.onToggleStepCollapsed}
											onJumpToStep={props.onJumpToStep}
											onJumpToAgent={props.onJumpToAgent}
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
