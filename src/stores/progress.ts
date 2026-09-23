import { batch, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { repositoriesStore } from "./repositories";
import { terminalsStore } from "./terminals";
import { toastsStore } from "./toasts";

export type ProgressKind = "done" | "blocked" | "intent" | "delegated" | "message";

export interface ProgressEntry {
	id: number;
	project: string;
	ptyId?: string;
	createdAtMs: number;
	type: ProgressKind;
	text: string;
	step?: string;
	agentName?: string;
	/** The terminal a `delegated` or `message` entry points at. */
	targetPtyId?: string;
	targetName?: string;
}

export type FlowState = "busy" | "idle" | "awaiting" | "closed" | "running" | "done";

export interface FlowParticipant {
	id: string;
	kind: "terminal" | "subagent";
	title: string;
	agentType?: string;
	state: FlowState;
	parent?: string;
	intent?: string;
	toolCalls: number;
	ptyId: string;
	agentId?: string;
}

export interface FlowDetailRef {
	ptyId: string;
	agentId: string;
	part: "prompt" | "report";
}

export type FlowEventKind =
	| "intent"
	| "done"
	| "blocked"
	| "delegated"
	| "message"
	| "subagent_spawn"
	| "subagent_return";

export interface FlowEvent {
	kind: FlowEventKind;
	from: string;
	to?: string;
	summary: string;
	/** The whole journal text, when the summary is shorter. */
	text?: string;
	/** Where the whole text of a subagent arrow is fetched from. */
	detail?: FlowDetailRef;
	step?: string;
	atMs: number;
}

export interface ProgressFlow {
	project: string;
	participants: FlowParticipant[];
	events: FlowEvent[];
	truncated: boolean;
}

export type ProgressView = "list" | "flow";

export interface ProjectFlowState {
	data?: ProgressFlow;
	loading: boolean;
	error: string | null;
}

export interface ProgressList {
	project: string;
	entries: ProgressEntry[];
	ptyIds: string[];
	lastViewedMs?: number;
}

export interface ProgressRecordedPayload {
	repo_path: string;
	payload: { entry: ProgressEntry };
}

export interface ProjectProgressState {
	entries: ProgressEntry[];
	ptyIds: string[];
	/// The divider position, frozen when the dialog opened. The stored value
	/// moves on close; redrawing the line under the reader's cursor while they
	/// are still reading is the one thing it must not do.
	dividerMs?: number;
	loading: boolean;
	error: string | null;
}

const [dialogVisible, setDialogVisible] = createSignal(false);
const [requestedProject, setRequestedProject] = createSignal<string | null>(null);
const [selectedPtyId, setSelectedPtyId] = createSignal<string | null>(null);
const [blockedOnly, setBlockedOnly] = createSignal(false);
const [view, setView] = createSignal<ProgressView>("list");

function messageOf(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

export function createProgressStore() {
	const [state, setState] = createStore<{
		projects: Record<string, ProjectProgressState>;
		flows: Record<string, ProjectFlowState>;
	}>({ projects: {}, flows: {} });
	// Entries that arrived while the dialog was closed. The bell shows this
	// count. It is deliberately NOT a query across every registered repository:
	// that fan-out is what made the old panel fire 78 requests on open, and it
	// answered with one red block per repository that no longer existed.
	const [arrivedSinceOpen, setArrivedSinceOpen] = createSignal(0);
	const visitedScopes = new Set<string | null>();

	function ensure(project: string): void {
		if (state.projects[project]) return;
		setState("projects", project, { entries: [], ptyIds: [], loading: false, error: null });
	}

	async function refreshProject(project: string, freezeDivider = true): Promise<void> {
		ensure(project);
		const ptyId = selectedPtyId();
		setState("projects", project, { loading: true, error: null });
		try {
			const list = await invoke<ProgressList>("progress_list", {
				project,
				input: { blockedOnly: blockedOnly(), ...(ptyId ? { ptyId } : {}) },
			});
			if (requestedProject() !== project || selectedPtyId() !== ptyId) return;
			const frozen = freezeDivider ? (state.projects[project]?.dividerMs ?? list.lastViewedMs) : list.lastViewedMs;
			setState("projects", project, { loading: false, error: null, dividerMs: frozen });
			setState("projects", project, "entries", reconcile(list.entries));
			setState("projects", project, "ptyIds", reconcile(list.ptyIds ?? []));
		} catch (error) {
			if (requestedProject() !== project || selectedPtyId() !== ptyId) return;
			setState("projects", project, { loading: false, error: messageOf(error) });
		}
	}

	/// The Flow view's read. The backend builds the whole sequence — columns,
	/// order, arrows, redaction — so this only stores what came back.
	async function refreshFlow(project: string): Promise<void> {
		const ptyId = selectedPtyId();
		setState("flows", project, (current) => ({ ...(current ?? {}), loading: true, error: null }));
		try {
			const flow = await invoke<ProgressFlow>("progress_flow", {
				project,
				input: ptyId ? { ptyId } : {},
			});
			if (requestedProject() !== project || selectedPtyId() !== ptyId) return;
			setState("flows", project, { data: flow, loading: false, error: null });
		} catch (error) {
			if (requestedProject() !== project || selectedPtyId() !== ptyId) return;
			setState("flows", project, { loading: false, error: messageOf(error) });
		}
	}

	/// Refresh whichever view is showing. The list stays the source of the
	/// divider and of deletion, so it is read in both.
	function refreshVisible(project: string, freezeDivider = true): void {
		void refreshProject(project, freezeDivider);
		if (view() === "flow") void refreshFlow(project);
	}

	function open(project: string | null = null, ptyId?: string | null): void {
		const target = project ?? repositoriesStore.state.activeRepoPath ?? null;
		const activeId = terminalsStore.state.activeId;
		const active = activeId ? terminalsStore.state.terminals[activeId] : undefined;
		const selected = ptyId === undefined ? (active?.repoPath === target ? active.sessionId : null) : ptyId;
		visitedScopes.clear();
		visitedScopes.add(selected ?? null);
		setRequestedProject(target);
		setSelectedPtyId(selected ?? null);
		setArrivedSinceOpen(0);
		setDialogVisible(true);
		if (target) {
			// A project opened for the first time in this session has no frozen
			// divider yet, so the stored one is adopted here and held until close.
			setState("projects", target, (current) => ({
				...(current ?? { entries: [], ptyIds: [], loading: false, error: null }),
			}));
			setState("projects", target, "entries", []);
			refreshVisible(target, false);
		}
	}

	function selectPty(ptyId: string | null): void {
		visitedScopes.add(ptyId);
		setSelectedPtyId(ptyId);
		const project = requestedProject();
		if (project) {
			setState("projects", project, "entries", []);
			setState("projects", project, "dividerMs", undefined);
			refreshVisible(project, false);
		}
	}

	async function close(): Promise<void> {
		const project = requestedProject();
		setDialogVisible(false);
		if (!project) return;
		let failure: unknown;
		for (const ptyId of visitedScopes) {
			try {
				await invoke("progress_mark_viewed", { project, ...(ptyId ? { ptyId } : {}) });
			} catch (error) {
				failure ??= error;
			}
		}
		setState("projects", project, "dividerMs", undefined);
		if (failure !== undefined) {
			// Failing to move the divider costs the reader a stale line, never an
			// entry. It is not worth a toast.
			setState("projects", project, "error", messageOf(failure));
		}
	}

	async function deleteEntries(project: string, ids: number[]): Promise<boolean> {
		ensure(project);
		setState("projects", project, "error", null);
		try {
			await invoke("progress_delete", { project, input: { ids } });
			await refreshProject(project);
			return true;
		} catch (error) {
			setState("projects", project, "error", messageOf(error));
			return false;
		}
	}

	function presentLive(payload: ProgressRecordedPayload): void {
		const entry = payload.payload.entry;
		if (
			dialogVisible() &&
			requestedProject() === payload.repo_path &&
			(selectedPtyId() === null || selectedPtyId() === entry.ptyId)
		) {
			refreshVisible(payload.repo_path);
		} else {
			setArrivedSinceOpen((count) => count + 1);
		}
		// An `intent:` is what the agent set out to do, and a hand-off is one
		// agent talking to another — neither is an outcome. They belong in the
		// journal and not in the user's face.
		if (entry.type !== "done" && entry.type !== "blocked") return;
		const projectName =
			repositoriesStore.get(payload.repo_path)?.displayName ??
			payload.repo_path.split(/[\\/]/).pop() ??
			payload.repo_path;
		toastsStore.add(
			// The repo badge already names the project (it is passed below), so a
			// title of "<project> · <step>" printed it twice and spent half the
			// title's width doing it. The project name is the title only when
			// there is no step to put there.
			entry.step ?? projectName,
			entry.text,
			entry.type === "blocked" ? "warn" : "info",
			// Silent by default: a blocked entry is not automatically a request
			// for the user's attention.
			false,
			{ label: "Open Progress", onClick: () => open(payload.repo_path, entry.ptyId ?? null) },
			undefined,
			payload.repo_path,
			undefined,
			false,
		);
	}

	return {
		state,
		dialogVisible,
		requestedProject,
		selectedPtyId,
		selectPty,
		blockedOnly,
		setBlockedOnly: (value: boolean) => {
			setBlockedOnly(value);
			const project = requestedProject();
			if (project) void refreshProject(project);
		},
		view,
		setView: (next: ProgressView) => {
			setView(next);
			const project = requestedProject();
			if (next === "flow" && project) void refreshFlow(project);
		},
		refreshFlow,
		fetchFlowDetail: (detail: FlowDetailRef) =>
			invoke<{ text: string }>("progress_flow_detail", { input: detail }).then((result) => result.text),
		open,
		close,
		toggle: () => (dialogVisible() ? void close() : open()),
		refreshProject,
		deleteEntries,
		presentLive,
		get unreadCount() {
			return arrivedSinceOpen();
		},
		resetForTests() {
			batch(() => {
				setState("projects", reconcile({}));
				setState("flows", reconcile({}));
				setView("list");
				setDialogVisible(false);
				setRequestedProject(null);
				setSelectedPtyId(null);
				setBlockedOnly(false);
				setArrivedSinceOpen(0);
				visitedScopes.clear();
			});
		},
	};
}

export const progressStore = createProgressStore();
