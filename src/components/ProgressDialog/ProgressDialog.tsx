import { type Component, createMemo, For, onMount, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { type ProgressEntry, type ProgressKind, progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { formatRelativeTime } from "../../utils/time";
import d from "../shared/dialog.module.css";
import s from "./ProgressDialog.module.css";

const KIND_ICON: Record<ProgressKind, string> = {
	// A check, a warning triangle and a small arrow. Monochrome paths, never emoji.
	done: "M20 6 9 17l-5-5",
	blocked: "M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
	intent: "M5 12h14m-6-6 6 6-6 6",
};

const KindIcon: Component<{ kind: ProgressKind }> = (props) => (
	<svg
		class={s.icon}
		width="14"
		height="14"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="2"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<path d={KIND_ICON[props.kind]} />
	</svg>
);

export interface ProgressDialogProps {
	/**
	 * Fill the parent instead of floating over it. The mobile shell gives
	 * Progress a whole bottom tab, which is already the modal surface a dialog
	 * would create — so it drops the overlay and keeps the list.
	 */
	embedded?: boolean;
}

/**
 * The Progress journal for one project: one newest-first list, a line marking
 * where the last visit ended, and nothing else. There are no pages, no tabs and
 * no export — the plan's revision removed all three on purpose.
 */
export const ProgressDialog: Component<ProgressDialogProps> = (props) => {
	if (!props.embedded) registerModal(() => void progressStore.close());
	// Embedded, nothing opened the dialog, so nothing chose the project either.
	onMount(() => {
		if (props.embedded && !progressStore.requestedProject()) progressStore.open();
	});

	const project = () => progressStore.requestedProject();
	const projectState = () => {
		const path = project();
		return path ? progressStore.state.projects[path] : undefined;
	};
	const entries = createMemo<ProgressEntry[]>(() => projectState()?.entries ?? []);

	const displayName = () => {
		const path = project();
		if (!path) return "";
		return repositoriesStore.get(path)?.displayName ?? path.split(/[\\/]/).pop() ?? path;
	};

	/// The index the divider is drawn above: the first entry older than the
	/// frozen last-visit timestamp. `-1` means every entry is new, `entries
	/// .length` means none is, and either way the divider is simply not drawn.
	const dividerIndex = createMemo(() => {
		const since = projectState()?.dividerMs;
		if (since === undefined) return -1;
		const index = entries().findIndex((entry) => entry.createdAtMs <= since);
		return index <= 0 ? -1 : index;
	});

	const body = (
		<div
			class={`${s.dialog}${props.embedded ? ` ${s.embedded}` : ""}`}
			role={props.embedded ? "region" : "dialog"}
			aria-label="Project Progress"
			onClick={(e) => e.stopPropagation()}
		>
			<div class={s.header}>
				<div class={s.title}>
					<h4>Progress</h4>
					<span class={s.project}>{displayName()}</span>
				</div>
				<label class={s.filter}>
					<input
						type="checkbox"
						checked={progressStore.blockedOnly()}
						onChange={(e) => progressStore.setBlockedOnly(e.currentTarget.checked)}
					/>
					Blocked only
				</label>
				<Show when={!props.embedded}>
					<button type="button" class={s.iconButton} aria-label="Close" onClick={() => void progressStore.close()}>
						<svg
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							aria-hidden="true"
						>
							<path d="M18 6 6 18M6 6l12 12" />
						</svg>
					</button>
				</Show>
			</div>

			<Show when={projectState()?.error}>
				{/* One line for the whole failure. The old panel printed one red block
					    per repository and filled the view with them. */}
				<p class={s.error}>{projectState()?.error}</p>
			</Show>

			<div class={s.list}>
				<Show
					when={entries().length > 0}
					fallback={
						<p class={s.empty}>
							{/* A journal that could not be read is unavailable. Saying
							    "nothing recorded" there would report a failure as a
							    finished, empty project. */}
							{projectState()?.error
								? "This project's journal is unavailable."
								: projectState()?.loading
									? "Loading…"
									: progressStore.blockedOnly()
										? "Nothing is blocked."
										: "No progress recorded for this project yet."}
						</p>
					}
				>
					<For each={entries()}>
						{(entry, index) => (
							<>
								<Show when={index() === dividerIndex()}>
									<div class={s.divider}>Seen before</div>
								</Show>
								<div class={s.entry} data-kind={entry.type}>
									<KindIcon kind={entry.type} />
									<div class={s.body}>
										<p class={s.text}>{entry.text}</p>
										<div class={s.meta}>
											<Show when={entry.type === "intent"}>
												<span class={s.kindLabel}>set out to</span>
												<span>·</span>
											</Show>
											<span>{formatRelativeTime(entry.createdAtMs, { showDateFallback: true })}</span>
											<Show when={entry.step}>
												<span>· {entry.step}</span>
											</Show>
											<Show when={entry.agentName}>
												<span>· {entry.agentName}</span>
											</Show>
										</div>
									</div>
									<button
										type="button"
										class={s.delete}
										aria-label="Delete entry"
										onClick={() => {
											const path = project();
											if (path) void progressStore.deleteEntries(path, [entry.id]);
										}}
									>
										<svg
											width="14"
											height="14"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											stroke-width="2"
											stroke-linecap="round"
											aria-hidden="true"
										>
											<path d="M3 6h18M8 6V4h8v2m-9 0 1 14h8l1-14" />
										</svg>
									</button>
								</div>
							</>
						)}
					</For>
				</Show>
			</div>
		</div>
	);

	return (
		<Show when={!props.embedded} fallback={body}>
			{/* Clicking the backdrop dismisses; the dialog itself stops the event. */}
			<div class={d.overlay} onClick={() => void progressStore.close()}>
				{body}
			</div>
		</Show>
	);
};

export default ProgressDialog;
