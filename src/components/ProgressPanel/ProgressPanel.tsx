import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { type ProgressEvent, type ProgressKind, progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import s from "./ProgressPanel.module.css";

type View = "since" | "today" | "blockers" | "completed" | "history";

function projectName(path: string): string {
	return repositoriesStore.get(path)?.displayName ?? path.split(/[\\/]/).pop() ?? path;
}

function startOfToday(): number {
	const date = new Date();
	date.setHours(0, 0, 0, 0);
	return date.getTime();
}

function queryForView(view: View) {
	switch (view) {
		case "since":
			return { unreadOnly: true };
		case "today":
			return { createdAfterMs: startOfToday() };
		case "blockers":
			return { blockerOnly: true };
		case "completed":
			return { kind: "done" as const };
		default:
			return {};
	}
}

function kindLabel(kind: ProgressKind): string {
	return { started: "Started", milestone: "Milestone", blocked: "Blocked", done: "Completed" }[kind];
}

function formatTime(timestamp: number): string {
	return new Intl.DateTimeFormat(undefined, {
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
	}).format(timestamp);
}

export interface ProgressPanelProps {
	embedded?: boolean;
	onClose?: () => void;
}

export function ProgressPanel(props: ProgressPanelProps = {}) {
	const [scope, setScope] = createSignal<string>("global");
	const [view, setView] = createSignal<View>("since");
	const [selected, setSelected] = createSignal<Set<string>>(new Set());
	const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
	createEffect(
		on(
			() => [scope(), view()] as const,
			() => setSelected(new Set<string>()),
			{ defer: true },
		),
	);

	createEffect(
		on(
			() => props.embedded || progressStore.panelVisible(),
			(visible) => {
				if (!visible) return;
				const requested = progressStore.requestedProject();
				setScope(requested && repositoriesStore.get(requested) ? requested : "global");
				if (progressStore.requestedEvent()) setView("history");
			},
		),
	);

	let alreadyOpen = false;
	createEffect(
		on(
			() =>
				[
					props.embedded || progressStore.panelVisible(),
					view(),
					...repositoriesStore.getPaths().map((path) => repositoriesStore.getRevision(path)),
				] as const,
			([visible]) => {
				if (!visible) {
					alreadyOpen = false;
					return;
				}
				const preserveWatermark = alreadyOpen;
				alreadyOpen = true;
				void progressStore.refreshAll(preserveWatermark, queryForView(view())).then(() => {
					const eventId = progressStore.requestedEvent();
					if (eventId)
						requestAnimationFrame(() =>
							document.getElementById(`progress-${eventId}`)?.scrollIntoView({ block: "center" }),
						);
				});
			},
		),
	);

	const scopedProjects = createMemo(() =>
		scope() === "global" ? repositoriesStore.getPaths() : repositoriesStore.get(scope()) ? [scope()] : [],
	);
	const rows = createMemo(() => {
		const result: Array<{ project: string; event: ProgressEvent }> = [];
		for (const project of scopedProjects()) {
			const entry = progressStore.state.projects[project];
			const status = entry?.status;
			for (const event of entry?.events ?? []) {
				if (status && event.sequence > status.snapshotCursor) continue;
				result.push({ project, event });
			}
		}
		return result.sort((a, b) => b.event.createdAtMs - a.event.createdAtMs || b.event.sequence - a.event.sequence);
	});
	const selectedInProject = (project: string) =>
		rows().filter((row) => row.project === project && selected().has(row.event.id));

	async function markViewed(): Promise<void> {
		await Promise.all(
			scopedProjects().map((project) => {
				const cursor = progressStore.state.projects[project]?.status?.snapshotCursor;
				return cursor === undefined ? Promise.resolve(false) : progressStore.markRead(project, cursor);
			}),
		);
	}

	async function deleteSelected(): Promise<void> {
		const count = selected().size;
		if (!count) return;
		const affectedScopes = scopedProjects()
			.map((project) => {
				const projectCount = selectedInProject(project).length;
				return projectCount ? `${projectName(project)} (${projectCount})` : undefined;
			})
			.filter((item): item is string => item !== undefined)
			.join(", ");
		if (
			!window.confirm(
				`Delete ${count} selected Progress event${count === 1 ? "" : "s"} from ${affectedScopes}? Existing progress.md exports are not deleted.`,
			)
		)
			return;
		for (const project of scopedProjects()) {
			const ids = selectedInProject(project).map((row) => row.event.id);
			if (ids.length) await progressStore.deleteEvents(project, ids);
		}
		setSelected(new Set<string>());
	}

	async function clearProject(): Promise<void> {
		const project = scope();
		if (project === "global") return;
		const status = progressStore.state.projects[project]?.status;
		if (!status) return;
		if (
			!window.confirm(
				`Clear all Progress history for ${projectName(project)} and pause collection? Existing progress.md exports are not deleted.`,
			)
		)
			return;
		await progressStore.clear(project, status.revision);
	}

	async function editEvent(project: string, event: ProgressEvent): Promise<void> {
		const summary = window.prompt("Correct Progress summary", event.summary)?.trim();
		if (!summary || summary === event.summary) return;
		const revision = progressStore.state.projects[project]?.status?.revision;
		if (revision !== undefined)
			await progressStore.correct(project, revision, [{ operation: "edit_summary", eventId: event.id, summary }]);
	}

	async function mergeSelected(): Promise<void> {
		const project = scope();
		if (project === "global") return;
		const ids = selectedInProject(project).map((row) => row.event.id);
		if (ids.length < 2) return;
		const targetEventId = ids[0];
		const revision = progressStore.state.projects[project]?.status?.revision;
		if (revision === undefined) return;
		if (
			await progressStore.correct(project, revision, [
				{ operation: "merge_events", sourceEventIds: ids.slice(1), targetEventId },
			])
		)
			setSelected(new Set<string>());
	}

	async function renameWorkstream(project: string, workstreamId: string, current: string): Promise<void> {
		const name = window.prompt("Rename workstream", current)?.trim();
		const revision = progressStore.state.projects[project]?.status?.revision;
		if (name && name !== current && revision !== undefined)
			await progressStore.correct(project, revision, [{ operation: "rename_workstream", workstreamId, name }]);
	}

	async function mergeWorkstream(project: string, sourceId: string): Promise<void> {
		const status = progressStore.state.projects[project]?.status;
		const target = window.prompt("Target workstream name")?.trim();
		const targetWorkstream = status?.workstreams.find((item) => item.name === target && item.id !== sourceId);
		if (!status || !targetWorkstream) return;
		await progressStore.correct(project, status.revision, [
			{ operation: "merge_workstreams", sourceWorkstreamIds: [sourceId], targetWorkstreamId: targetWorkstream.id },
		]);
	}

	async function moveEvent(project: string, event: ProgressEvent): Promise<void> {
		const status = progressStore.state.projects[project]?.status;
		const target = window.prompt("Move to workstream (leave empty for Project)", event.workstream ?? "")?.trim();
		if (!status || target === undefined) return;
		const targetWorkstream = target ? status.workstreams.find((item) => item.name === target) : undefined;
		if (target && !targetWorkstream) return;
		await progressStore.correct(project, status.revision, [
			{ operation: "move_event", eventId: event.id, workstreamId: targetWorkstream?.id ?? null },
		]);
	}

	return (
		<Show when={props.embedded || progressStore.panelVisible()}>
			<aside
				id={props.embedded ? "mobile-progress-panel" : "progress-panel"}
				class={s.panel}
				classList={{ [s.embedded]: !!props.embedded }}
				aria-label="Project Progress"
			>
				<Show when={!props.embedded}>
					<PanelResizeHandle panelId="progress-panel" minWidth={320} maxWidth={900} />
				</Show>
				<header class={s.header}>
					<div>
						<strong>Progress</strong>
						<span>What meaningfully changed</span>
					</div>
					<Show when={!props.embedded || props.onClose}>
						<button
							class={s.iconButton}
							onClick={() => props.onClose?.() ?? progressStore.close()}
							aria-label="Close Progress"
						>
							×
						</button>
					</Show>
				</header>
				<div class={s.scopeRow}>
					<select
						value={scope()}
						onChange={(event) => {
							setScope(event.currentTarget.value);
							setSelected(new Set<string>());
						}}
						aria-label="Progress scope"
					>
						<option value="global">All projects</option>
						<For each={repositoriesStore.getPaths()}>{(path) => <option value={path}>{projectName(path)}</option>}</For>
					</select>
					<button onClick={() => void markViewed()}>Mark viewed</button>
				</div>
				<nav class={s.tabs} aria-label="Progress views">
					<For
						each={
							[
								["since", "Since last visit"],
								["today", "Today"],
								["blockers", "Blockers"],
								["completed", "Completed"],
								["history", "History"],
							] as Array<[View, string]>
						}
					>
						{([id, label]) => (
							<button classList={{ [s.active]: view() === id }} onClick={() => setView(id)}>
								{label}
							</button>
						)}
					</For>
				</nav>
				<Show when={scope() !== "global" && progressStore.state.projects[scope()]?.status}>
					{(status) => (
						<section class={s.stateCard}>
							<div>
								<strong>{projectName(scope())}</strong>
								<span>{status().collectionEnabled ? "Collecting" : "Paused"}</span>
							</div>
							<div class={s.controlRow}>
								<button
									onClick={() =>
										void (status().collectionEnabled ? progressStore.pause(scope()) : progressStore.resume(scope()))
									}
								>
									{status().collectionEnabled ? "Pause" : "Resume"}
								</button>
								<button disabled={selected().size === 0} onClick={() => void deleteSelected()}>
									Delete ({selected().size})
								</button>
								<button disabled={selected().size < 2} onClick={() => void mergeSelected()}>
									Merge
								</button>
								<button class={s.danger} onClick={() => void clearProject()}>
									Clear project
								</button>
							</div>
							<div class={s.workstreams}>
								<For each={status().workstreams}>
									{(workstream) => (
										<div class={s.workstream}>
											<span>{workstream.name}</span>
											<select
												value={workstream.state}
												aria-label={`State for ${workstream.name}`}
												onChange={(event) =>
													void progressStore.correct(scope(), status().revision, [
														{
															operation: "set_workstream_state",
															workstreamId: workstream.id,
															state: event.currentTarget.value as import("../../stores/progress").WorkstreamState,
														},
													])
												}
											>
												<option value="started">started</option>
												<option value="progressing">progressing</option>
												<option value="blocked">blocked</option>
												<option value="done">done</option>
											</select>
											<button onClick={() => void renameWorkstream(scope(), workstream.id, workstream.name)}>
												Rename
											</button>
											<button onClick={() => void mergeWorkstream(scope(), workstream.id)}>Merge into…</button>
											<Show when={workstream.activeBlockers > 0}>
												<button
													onClick={() => {
														const blocker = progressStore.state.projects[scope()]?.events.find(
															(event) => event.workstreamId === workstream.id && event.type === "blocked",
														);
														const revision = progressStore.state.projects[scope()]?.status?.revision;
														if (blocker && revision !== undefined)
															void progressStore.correct(scope(), revision, [
																{ operation: "resolve_blocker", eventId: blocker.id },
															]);
													}}
												>
													Resolve blocker
												</button>
											</Show>
										</div>
									)}
								</For>
							</div>
						</section>
					)}
				</Show>
				<div class={s.content}>
					<For each={scopedProjects()}>
						{(project) => (
							<Show when={progressStore.state.projects[project]?.error}>
								{(error) => (
									<div class={s.error}>
										<strong>{projectName(project)}</strong>: {error()}
									</div>
								)}
							</Show>
						)}
					</For>
					<Show when={rows().length > 0} fallback={<div class={s.empty}>No progress matches this view.</div>}>
						<For each={rows()}>
							{({ project, event }) => (
								<article class={s.event} id={`progress-${event.id}`}>
									<input
										type="checkbox"
										checked={selected().has(event.id)}
										onChange={(input) =>
											setSelected((current) => {
												const next = new Set(current);
												input.currentTarget.checked ? next.add(event.id) : next.delete(event.id);
												return next;
											})
										}
										aria-label={`Select ${event.summary}`}
									/>
									<div class={s.eventBody}>
										<div class={s.primary}>
											<strong>{projectName(project)}</strong>
											<span>›</span>
											<span>{event.workstream ?? "Project"}</span>
											<span class={s.kind} data-kind={event.type}>
												{kindLabel(event.type)}
											</span>
										</div>
										<p>{event.summary}</p>
										<div class={s.meta}>
											<time>{formatTime(event.createdAtMs)}</time>
											<button
												onClick={() =>
													setExpanded((current) => {
														const next = new Set(current);
														next.has(event.id) ? next.delete(event.id) : next.add(event.id);
														return next;
													})
												}
											>
												Source
											</button>
											<button onClick={() => void editEvent(project, event)}>Correct</button>
											<button onClick={() => void moveEvent(project, event)}>Move</button>
										</div>
										<Show when={expanded().has(event.id)}>
											<div class={s.provenance}>
												{event.reporterName ?? event.reporterId ?? "Unknown reporter"}
												{event.workspacePath ? ` · ${event.workspacePath}` : ""}
												<Show when={event.sessionId}>
													<button
														disabled={!progressStore.isSourceLive(event)}
														onClick={() => progressStore.openSource(event)}
													>
														{progressStore.isSourceLive(event) ? "Open exact session" : "Session closed"}
													</button>
												</Show>
											</div>
										</Show>
									</div>
								</article>
							)}
						</For>
					</Show>
					<For each={scopedProjects()}>
						{(project) => (
							<Show when={progressStore.state.projects[project]?.nextBeforeSequence}>
								<button class={s.loadMore} onClick={() => void progressStore.loadMore(project)}>
									Load older {projectName(project)} events
								</button>
							</Show>
						)}
					</For>
				</div>
			</aside>
		</Show>
	);
}
