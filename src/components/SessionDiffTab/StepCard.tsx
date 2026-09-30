import { type Component, createEffect, createMemo, createSignal, Show } from "solid-js";
import { CommentBox } from "../../components/DiffTab/CommentBox";
import { extractHunks, extractSelectedLines } from "../../components/DiffTab/diffPatch";
import { sendDiffComment } from "../../components/DiffTab/sendDiffComment";
import { createLineSelection } from "../../components/DiffTab/useLineSelection";
import { usePty } from "../../hooks/usePty";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";
import type { DiffViewMode } from "../../stores/ui";
import type { EditStep } from "../../types/sessionDiff";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import fl from "../shared/diffFileList.module.css";
import { DiffViewer } from "../ui/DiffViewer";
import s from "./SessionDiffTab.module.css";

export interface StepCardProps {
	step: EditStep;
	mode: DiffViewMode;
	/** Show the file path in this card's own header — used in the flat
	 *  chronological view; the grouped-by-file view already shows it once. */
	showFilePath?: boolean;
	/** Whether this step's diff is hidden. Omit (falsy) for the pre-existing
	 *  "always expanded" behavior — every current caller either passes it or
	 *  doesn't care, so this stays backward-compatible. */
	collapsed?: boolean;
	/** Present whenever the caller wants the collapse caret at all — makes the
	 *  whole header row clickable, not just a small chevron. */
	onToggleCollapsed?: () => void;
	/** `^`/`v` "jump to the other time this file changed" buttons, shown only
	 *  alongside `showFilePath` — `undefined` hides the button entirely
	 *  (there's no earlier/later touch of this file to jump to). */
	onJumpPrev?: () => void;
	onJumpNext?: () => void;
	onOpenAtLine: (step: EditStep, line: number) => void;
	onRevertStep: (step: EditStep) => void;
	onCopyStep: (step: EditStep) => void;
	/** Present only when the session's `tuic_session_id` resolves to a live
	 *  terminal tab — a subagent step jumps to its PARENT session's tab (it
	 *  has no PTY of its own), so this is the same handler for every step in
	 *  a review, computed once by `SessionDiffTab`, not derived per-step. */
	onJumpToAgent?: () => void;
}

const KIND_LABEL: Record<EditStep["kind"], string> = { create: "created", overwrite: "overwrote", edit: "edited" };

/** One edit step's diff — used both nested under a file header (grouped view)
 *  and standalone (flat chronological view). Owns its own line-selection and
 *  comment-box state, since a drag selection is scoped to one rendered diff. */
export const StepCard: Component<StepCardProps> = (props) => {
	const pty = usePty();
	const hunks = createMemo(() => extractHunks(props.step.patch));
	const lineSelection = createLineSelection({ hunks, selectedClass: s.lineSelected });
	const selectedCount = () => lineSelection.selectedLines().size;

	// DiffViewer rebuilds its <tr> rows whenever `mode` (unified/split)
	// changes, so the cached row map must be dropped too — otherwise
	// findLineInfo keeps resolving against detached rows and click/drag
	// selection silently stops working until a full remount (mirrors
	// DiffTab.tsx's identical invalidate-on-mode-change effect).
	createEffect(() => {
		props.mode;
		lineSelection.invalidate();
	});

	const [commentVisible, setCommentVisible] = createSignal(false);
	const [commentText, setCommentText] = createSignal("");
	const [commentError, setCommentError] = createSignal<string | null>(null);

	async function handleCommentSend() {
		const hIdx = lineSelection.selectedHunkIdx();
		if (hIdx === null) return;
		const { lines, startLine, endLine } = extractSelectedLines(props.step.patch, hIdx, lineSelection.selectedLines());
		const term = terminalsStore.findTerminalWithSession();

		const result = await sendDiffComment(
			{ filePath: props.step.rel_path ?? props.step.abs_path, startLine, endLine, lines },
			commentText(),
			term?.sessionId,
			term?.agentType,
			(sessionId, message, agentType) => pty.sendCommand(sessionId, message, agentType),
		);

		if (result === "ok") {
			setCommentText("");
			setCommentError(null);
			setCommentVisible(false);
			lineSelection.clear();
		} else if (result === "no-terminal") {
			setCommentError("No terminal with active session — open a terminal first");
		} else if (result === "error") {
			appLogger.error("git", "Failed to send comment to terminal");
			setCommentError("Failed to send — see logs for details");
		}
	}

	const timeLabel = () => {
		if (!props.step.timestamp) return "";
		const d = new Date(props.step.timestamp);
		return Number.isNaN(d.getTime()) ? "" : d.toLocaleTimeString();
	};

	return (
		<div class={s.stepCard}>
			<div
				class={s.stepHeader}
				role="button"
				tabIndex={0}
				onClick={props.onToggleCollapsed}
				onKeyDown={onClickKeyDown(props.onToggleCollapsed ?? (() => {}))}
			>
				<svg
					class={cx(fl.chevron, props.collapsed && fl.chevronCollapsed)}
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="currentColor"
				>
					<path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
				</svg>
				<span class={s.stepKind}>{KIND_LABEL[props.step.kind]}</span>
				<Show when={props.showFilePath}>
					<span class={s.stepPath}>{props.step.rel_path ?? props.step.abs_path}</span>
					<Show when={props.onJumpPrev}>
						{(onJumpPrev) => (
							<button
								type="button"
								class={s.iconBtn}
								onClick={(e) => {
									e.stopPropagation();
									onJumpPrev()();
								}}
								title="Jump to the earlier change to this file"
							>
								<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
									<path d="M8 4l-5 6h10z" />
								</svg>
							</button>
						)}
					</Show>
					<Show when={props.onJumpNext}>
						{(onJumpNext) => (
							<button
								type="button"
								class={s.iconBtn}
								onClick={(e) => {
									e.stopPropagation();
									onJumpNext()();
								}}
								title="Jump to the later change to this file"
							>
								<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
									<path d="M8 12l5-6H3z" />
								</svg>
							</button>
						)}
					</Show>
				</Show>
				<Show when={props.step.is_sidechain}>
					{(() => {
						const displayName = () => props.step.agent_display_name ?? props.step.agent_name;
						const label = () => `subagent${displayName() ? `: ${displayName()}` : ""}`;
						return (
							<Show
								when={props.onJumpToAgent}
								fallback={
									<span class={s.badge} title={displayName() ? `Subagent: ${displayName()}` : "Subagent edit"}>
										{label()}
									</span>
								}
							>
								{(onJumpToAgent) => (
									<button
										type="button"
										class={cx(s.badge, s.badgeLink)}
										title={`Jump to ${displayName() ?? "this agent"}'s tab`}
										onClick={(e) => {
											e.stopPropagation();
											onJumpToAgent()();
										}}
									>
										{label()}
									</button>
								)}
							</Show>
						);
					})()}
				</Show>
				<Show when={timeLabel()}>
					<span class={s.stepTime}>{timeLabel()}</span>
				</Show>
				<span class={s.stepStats}>
					<Show when={props.step.additions > 0}>
						<span class={s.statAdd}>+{props.step.additions}</span>
					</Show>
					<Show when={props.step.deletions > 0}>
						<span class={s.statDel}>-{props.step.deletions}</span>
					</Show>
				</span>
				<div class={s.stepActions}>
					<button
						type="button"
						class={s.iconBtn}
						onClick={(e) => {
							e.stopPropagation();
							props.onCopyStep(props.step);
						}}
						title="Copy this step's diff"
					>
						<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
							<path d="M4 2a2 2 0 0 0-2 2v7h1.5V4a.5.5 0 0 1 .5-.5h7V2H4zm3 3a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h5a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2H7z" />
						</svg>
					</button>
					<button
						type="button"
						class={s.iconBtn}
						onClick={(e) => {
							e.stopPropagation();
							props.onRevertStep(props.step);
						}}
						title="Revert just this step"
					>
						<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
							<path d="M2 8a6 6 0 1 1 12 0A6 6 0 0 1 2 8zm6-4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM6.5 7.5l3-2v4l-3-2z" />
						</svg>
					</button>
					<button
						type="button"
						class={s.iconBtn}
						onClick={(e) => {
							e.stopPropagation();
							props.onOpenAtLine(props.step, hunks()[0] ? firstNewLine(hunks()[0]) : 1);
						}}
						title="Open file at this change"
					>
						<svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
							<path d="M12.1 1.3a1.5 1.5 0 0 1 2.1 0l.5.5a1.5 1.5 0 0 1 0 2.1L5.8 12.8l-3.5.9.9-3.5L12.1 1.3zM11 3.4 4.1 10.3l-.5 1.9 1.9-.5L12.4 4.8 11 3.4z" />
						</svg>
					</button>
				</div>
			</div>
			<Show when={!props.collapsed}>
				<div
					class={s.stepDiff}
					onMouseDown={lineSelection.handlers.onMouseDown}
					onMouseMove={lineSelection.handlers.onMouseMove}
					onMouseUp={lineSelection.handlers.onMouseUp}
				>
					<DiffViewer
						diff={props.step.patch}
						mode={props.mode}
						emptyMessage="No change (this step was a no-op)"
						contentRef={(el) => lineSelection.setContentRef(el)}
					/>
				</div>
				<Show when={selectedCount() > 0}>
					<div class={s.selectionBar}>
						<Show when={commentVisible()}>
							<CommentBox
								value={commentText()}
								error={commentError()}
								onInput={setCommentText}
								onSend={handleCommentSend}
								onCancel={() => setCommentVisible(false)}
							/>
						</Show>
						<button type="button" class={s.linkBtn} onClick={() => setCommentVisible(true)}>
							Comment on {selectedCount()} line{selectedCount() > 1 ? "s" : ""}
						</button>
						<button
							type="button"
							class={s.linkBtn}
							onClick={() => {
								lineSelection.clear();
								setCommentVisible(false);
							}}
						>
							Clear
						</button>
					</div>
				</Show>
			</Show>
		</div>
	);
};

/** First `+`-line's new-file line number in a hunk, for "open at this change". */
function firstNewLine(hunk: string): number {
	const match = hunk.match(/@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
	return match ? parseInt(match[1], 10) : 1;
}
