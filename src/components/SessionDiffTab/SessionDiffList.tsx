import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, For, type JSX, Show } from "solid-js";
import type { DiffViewMode } from "../../stores/ui";
import type { EditStep, FileReview } from "../../types/sessionDiff";
import fl from "../shared/diffFileList.module.css";
import { DiffViewer } from "../ui/DiffViewer";
import type { SessionRow } from "./buildRows";
import s from "./SessionDiffTab.module.css";
import { SessionFileHeader } from "./SessionFileHeader";
import { StepCard } from "./StepCard";

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
			return row?.kind === "file" ? `f:${row.group.abs_path}` : `s:${row?.step.tool_use_id}`;
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
						const row = props.rows[vi.index];
						return (
							<div
								data-index={vi.index}
								ref={(el) => virtualizer.measureElement(el)}
								style={{ position: "absolute", top: `${vi.start}px`, left: "0", width: "100%" }}
							>
								<Show when={row.kind === "file" ? row : null}>
									{(fileRow) => (
										<div class={fl.fileSection}>
											<SessionFileHeader
												group={fileRow().group}
												stepCount={fileRow().steps.length}
												expanded={fileRow().expanded}
												stepsOpen={fileRow().stepsOpen}
												onToggleExpanded={() => props.onToggleExpanded(fileRow().group.abs_path)}
												onToggleStepsOpen={() => props.onToggleStepsOpen(fileRow().group.abs_path)}
												onOpenFile={() => props.onOpenFile(fileRow().group.abs_path)}
												onRevertFile={() => props.onRevertFile(fileRow().group)}
												onCopyFile={() => props.onCopyFile(fileRow().group)}
											/>
											<Show when={fileRow().expanded}>
												<div class={fl.fileDiff}>
													<DiffViewer
														diff={fileRow().group.cumulative_patch}
														mode={props.mode}
														emptyMessage={
															fileRow().group.base_source === "unknown"
																? "Can't compute a cumulative diff for this file"
																: "No net change"
														}
													/>
												</div>
												<Show when={fileRow().stepsOpen}>
													<div class={s.nestedSteps}>
														<For each={fileRow().steps}>
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
								<Show when={row.kind === "step" ? row : null}>
									{(stepRow) => (
										<StepCard
											step={stepRow().step}
											mode={props.mode}
											showFilePath
											onOpenAtLine={props.onOpenAtLine}
											onRevertStep={props.onRevertStep}
											onCopyStep={props.onCopyStep}
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
