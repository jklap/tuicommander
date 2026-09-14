import { type Component, Show } from "solid-js";
import type { FileReview } from "../../types/sessionDiff";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import fl from "../shared/diffFileList.module.css";
import s from "./SessionDiffTab.module.css";

export interface SessionFileHeaderProps {
	group: FileReview;
	stepCount: number;
	expanded: boolean;
	stepsOpen: boolean;
	onToggleExpanded: () => void;
	onToggleStepsOpen: () => void;
	onOpenFile: () => void;
	onRevertFile: () => void;
	onCopyFile: () => void;
}

const BASE_SOURCE_LABEL: Record<FileReview["base_source"], string> = {
	backup: "exact — session-start backup",
	created_in_session: "created this session",
	tool_result: "exact — recorded by the tool",
	reconstructed: "reconstructed from current content",
	unknown: "unknown — can't compute a cumulative diff",
};

/** Per-file row header in the grouped-by-file view: chevron, path, +/- stats,
 *  a confidence/drift badge, and the file-level actions (copy, revert, open). */
export const SessionFileHeader: Component<SessionFileHeaderProps> = (props) => {
	return (
		<div class={fl.fileHeader}>
			<button
				type="button"
				class={s.chevronBtn}
				onClick={props.onToggleExpanded}
				onKeyDown={onClickKeyDown(props.onToggleExpanded)}
			>
				<svg
					class={cx(fl.chevron, !props.expanded && fl.chevronCollapsed)}
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="currentColor"
				>
					<path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
				</svg>
			</button>
			<span class={fl.filePath} onClick={props.onOpenFile}>
				{props.group.display_path}
			</span>
			<Show when={!props.group.in_repo}>
				<span class={s.badge} title="This file is outside the repository">
					outside repo
				</span>
			</Show>
			<Show when={props.group.drifted_from_disk}>
				<span class={cx(s.badge, s.badgeWarn)} title="Changed outside this session since it was last touched">
					drifted
				</span>
			</Show>
			<Show when={props.group.base_source === "unknown"}>
				<span class={cx(s.badge, s.badgeWarn)} title={BASE_SOURCE_LABEL.unknown}>
					unknown base
				</span>
			</Show>
			<span class={fl.fileStats}>
				<Show when={props.group.additions > 0}>
					<span class={fl.statAdd}>+{props.group.additions}</span>
				</Show>
				<Show when={props.group.deletions > 0}>
					<span class={fl.statDel}>-{props.group.deletions}</span>
				</Show>
			</span>
			<div class={s.fileActions}>
				<button type="button" class={s.iconBtn} onClick={props.onCopyFile} title="Copy this file's diff">
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M4 2a2 2 0 0 0-2 2v7h1.5V4a.5.5 0 0 1 .5-.5h7V2H4zm3 3a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h5a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2H7z" />
					</svg>
				</button>
				<Show when={props.group.base_source !== "unknown"}>
					<button
						type="button"
						class={s.iconBtn}
						onClick={props.onRevertFile}
						title="Revert this file to its session-start content"
					>
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M2 8a6 6 0 1 1 12 0A6 6 0 0 1 2 8zm6-4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM6.5 7.5l3-2v4l-3-2z" />
						</svg>
					</button>
				</Show>
				<Show when={props.stepCount > 1}>
					<button type="button" class={s.linkBtn} onClick={props.onToggleStepsOpen}>
						{props.stepsOpen ? "Hide" : "Show"} {props.stepCount} edits
					</button>
				</Show>
			</div>
		</div>
	);
};
