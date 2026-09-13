import { batch, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { repositoriesStore } from "./repositories";
import { terminalsStore } from "./terminals";
import { toastsStore } from "./toasts";

export type ProgressKind = "started" | "milestone" | "blocked" | "done";
export type WorkstreamState = "started" | "progressing" | "blocked" | "done";

export interface ProgressEvent {
	id: string;
	sequence: number;
	revision: number;
	createdAtMs: number;
	type: ProgressKind;
	summary: string;
	workstreamId?: string;
	workstream?: string;
	reporterId?: string;
	reporterName?: string;
	sessionId?: string;
	workspacePath?: string;
}

export interface WorkstreamSnapshot {
	id: string;
	name: string;
	state: WorkstreamState;
	activeBlockers: number;
	updatedSequence: number;
}

export interface ProgressStatus {
	projectRoot: string;
	revision: number;
	snapshotCursor: number;
	readCursor: number;
	unreadCount: number;
	collectionEnabled: boolean;
	workstreams: WorkstreamSnapshot[];
	projectBlockers: ProgressEvent[];
}

export interface ProgressPage {
	revision: number;
	snapshotCursor: number;
	events: ProgressEvent[];
	nextBeforeSequence?: number;
}

export interface ProgressExportReceipt {
	projectRoot: string;
	path: string;
	snapshotId: string;
	snapshotRevision: number;
	snapshotTimeMs: number;
	markdown: string;
	fileExists: boolean;
	existingContent?: string;
	written: boolean;
}

export type ProgressCorrection =
	| { operation: "edit_summary"; eventId: string; summary: string }
	| { operation: "move_event"; eventId: string; workstreamId: string | null }
	| { operation: "rename_workstream"; workstreamId: string; name: string }
	| { operation: "merge_workstreams"; sourceWorkstreamIds: string[]; targetWorkstreamId: string }
	| { operation: "merge_events"; sourceEventIds: string[]; targetEventId: string }
	| { operation: "resolve_blocker"; eventId: string }
	| { operation: "set_workstream_state"; workstreamId: string; state: WorkstreamState };

export interface ProgressRecordedPayload {
	repo_path: string;
	payload: { receipt: { status: string; revision: number; eventId?: string }; event: ProgressEvent };
}

export interface ProjectProgressState {
	status: ProgressStatus | null;
	events: ProgressEvent[];
	nextBeforeSequence?: number;
	loading: boolean;
	error: string | null;
	listInput: ProgressListInput;
}

export interface ProgressListInput {
	beforeSequence?: number;
	limit?: number;
	workstreamId?: string;
	kind?: ProgressKind;
	unreadOnly?: boolean;
	blockerOnly?: boolean;
	createdAfterMs?: number;
	createdBeforeMs?: number;
}

const PAGE_SIZE = 50;
const [panelVisible, setPanelVisible] = createSignal(false);
const [requestedProject, setRequestedProject] = createSignal<string | null>(null);
const [requestedEvent, setRequestedEvent] = createSignal<string | null>(null);

function messageOf(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

export function createProgressStore() {
	const [state, setState] = createStore<{ projects: Record<string, ProjectProgressState> }>({ projects: {} });
	const presented = new Set<string>();

	function ensure(project: string): void {
		if (state.projects[project]) return;
		setState("projects", project, { status: null, events: [], loading: false, error: null, listInput: {} });
	}

	async function refreshProject(
		project: string,
		preserveWatermark = false,
		requestedInput?: ProgressListInput,
	): Promise<void> {
		ensure(project);
		const listInput = requestedInput ?? state.projects[project]?.listInput ?? {};
		setState("projects", project, { loading: true, error: null });
		try {
			const [status, page] = await Promise.all([
				invoke<ProgressStatus>("progress_status", { project }),
				invoke<ProgressPage>("progress_list", { project, input: { ...listInput, limit: PAGE_SIZE } }),
			]);
			const frozen = preserveWatermark ? state.projects[project]?.status?.snapshotCursor : undefined;
			setState("projects", project, {
				status: frozen === undefined ? status : { ...status, snapshotCursor: frozen },
				nextBeforeSequence: page.nextBeforeSequence,
				loading: false,
				error: null,
				listInput,
			});
			setState("projects", project, "events", reconcile(page.events));
		} catch (error) {
			setState("projects", project, { loading: false, error: messageOf(error) });
		}
	}

	async function refreshAll(preserveWatermark = false, input?: ProgressListInput): Promise<void> {
		await Promise.all(repositoriesStore.getPaths().map((path) => refreshProject(path, preserveWatermark, input)));
	}

	async function loadMore(project: string): Promise<void> {
		const current = state.projects[project];
		if (!current?.nextBeforeSequence || current.loading) return;
		setState("projects", project, "loading", true);
		try {
			const page = await invoke<ProgressPage>("progress_list", {
				project,
				input: { ...current.listInput, beforeSequence: current.nextBeforeSequence, limit: PAGE_SIZE },
			});
			setState("projects", project, {
				nextBeforeSequence: page.nextBeforeSequence,
				loading: false,
			});
			setState("projects", project, "events", reconcile([...current.events, ...page.events]));
		} catch (error) {
			setState("projects", project, { loading: false, error: messageOf(error) });
		}
	}

	async function mutate(project: string, command: string, input?: Record<string, unknown>): Promise<boolean> {
		ensure(project);
		setState("projects", project, "error", null);
		try {
			await invoke(command, { project, ...(input ?? {}) });
			await refreshProject(project, true);
			return true;
		} catch (error) {
			setState("projects", project, "error", messageOf(error));
			return false;
		}
	}

	function open(project: string | null = null, eventId: string | null = null): void {
		setRequestedProject(project);
		setRequestedEvent(eventId);
		setPanelVisible(true);
	}

	function close(): void {
		setPanelVisible(false);
	}

	function presentLive(payload: ProgressRecordedPayload): void {
		const event = payload.payload.event;
		if (payload.payload.receipt.status !== "recorded" || presented.has(event.id)) return;
		presented.add(event.id);
		void refreshProject(payload.repo_path, panelVisible());
		const projectName =
			repositoriesStore.get(payload.repo_path)?.displayName ??
			payload.repo_path.split(/[\\/]/).pop() ??
			payload.repo_path;
		toastsStore.add(
			`${projectName} · ${event.workstream ?? "Project"}`,
			event.summary,
			event.type === "blocked" ? "warn" : "info",
			false,
			{ label: "Open Progress", onClick: () => open(payload.repo_path, event.id) },
			undefined,
			payload.repo_path,
			event.sessionId,
			false,
		);
	}

	function openSource(event: ProgressEvent): boolean {
		if (!event.sessionId) return false;
		const terminalId = terminalsStore.getTerminalForSession(event.sessionId);
		if (!terminalId) return false;
		const owner = repositoriesStore.findOwnerForTerminal(terminalId);
		if (owner) {
			repositoriesStore.setActive(owner.repoPath);
			repositoriesStore.setActiveWorkspace(owner.repoPath, owner.workspaceId);
		}
		terminalsStore.setActive(terminalId);
		close();
		return true;
	}

	return {
		state,
		panelVisible,
		requestedProject,
		requestedEvent,
		open,
		close,
		toggle: () => (panelVisible() ? close() : open()),
		refreshProject,
		refreshAll,
		loadMore,
		presentLive,
		openSource,
		isSourceLive: (event: ProgressEvent) =>
			!!event.sessionId && !!terminalsStore.getTerminalForSession(event.sessionId),
		pause: (project: string) => mutate(project, "progress_pause"),
		resume: (project: string) => mutate(project, "progress_resume"),
		deleteEvents: (project: string, eventIds: string[]) => mutate(project, "progress_delete", { input: { eventIds } }),
		clear: (project: string, expectedRevision: number) =>
			mutate(project, "progress_clear", { input: { expectedRevision } }),
		correct: (project: string, expectedRevision: number, corrections: ProgressCorrection[]) =>
			mutate(project, "progress_update", { input: { expectedRevision, corrections } }),
		markRead: async (project: string, snapshotCursor: number) => {
			const ok = await mutate(project, "progress_read", { input: { snapshotCursor } });
			if (ok) await refreshProject(project, true);
			return ok;
		},
		previewExport: (project: string, includeProvenance: boolean) =>
			invoke<ProgressExportReceipt>("progress_export", {
				project,
				input: { operation: "preview", options: { includeProvenance } },
			}),
		writeExport: (project: string, preview: ProgressExportReceipt, includeProvenance: boolean) =>
			invoke<ProgressExportReceipt>("progress_export", {
				project,
				input: {
					operation: "write",
					options: { includeProvenance },
					snapshotId: preview.snapshotId,
					snapshotTimeMs: preview.snapshotTimeMs,
					replace: preview.fileExists,
					expectedContent: preview.existingContent,
				},
			}),
		get unreadCount() {
			return Object.values(state.projects).reduce((sum, project) => sum + (project.status?.unreadCount ?? 0), 0);
		},
		resetForTests() {
			batch(() => {
				setState("projects", reconcile({}));
				presented.clear();
				setPanelVisible(false);
				setRequestedProject(null);
				setRequestedEvent(null);
			});
		},
	};
}

export const progressStore = createProgressStore();
