import { AGENT_TYPES, type AgentType } from "../agents";
import { handleAgentExitCompletion } from "../components/Terminal/agentExitCompletion";
import { invoke, listen } from "../invoke";
import { isNotificationSound } from "../notifications";
import { listenForNativeNoticeClicks } from "../services/nativeNotificationNavigation";
import { activityStore } from "../stores/activityStore";
import { appLogger } from "../stores/appLogger";
import { editorTabsStore } from "../stores/editorTabs";
import { githubStore } from "../stores/github";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../stores/globalWorkspace";
import { mdTabsStore, resolveRepoForCwd } from "../stores/mdTabs";
import { notificationsStore } from "../stores/notifications";
import { paneLayoutStore } from "../stores/paneLayout";
import { type ProgressRecordedPayload, progressStore } from "../stores/progress";
import { remoteConnectionsStore } from "../stores/remoteConnections";
import { repoSettingsStore } from "../stores/repoSettings";
import { placementWorkspaceFor, repositoriesStore, resolveRepoOwner, resolveRepoPathFor } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { reconcileTerminalOwnership } from "../stores/terminalOwnership";
import { terminalsStore } from "../stores/terminals";
import { toastsStore } from "../stores/toasts";
import { uiStore } from "../stores/ui";
import { workflowRunSignals } from "../stores/workflowRunSignals";
import { applyAppTheme, listenForThemeChanges, loadThemes } from "../themes";
import { isTauri, rpc, subscribeEvents } from "../transport";
import { getSessionConnection } from "../transportRuntime";
import type { RepoChangeKind, SavedTerminal } from "../types";
import { classifyFile, isImageFile } from "../utils/filePreview";
import { navigateToTerminal } from "../utils/navigateToTerminal";
import { assignTabToActiveGroup } from "../utils/paneTabAssign";
import { isAbsolutePath, pathStripPrefix } from "../utils/pathUtils";
import { sameDir, unregisteredRepoRootFor } from "../utils/repoOwnership";
import { isSuspendingOrSuspended, suspendTerminal } from "../utils/suspendTerminal";
import { createRevisionCoalescer } from "./revisionCoalescer";

/** Track PTY sessions created by the browser client so we only close our own on unload */
export const browserCreatedSessions = new Set<string>();

/** Remote (MCP) sessionId → termId. Persists even after Terminal.tsx nulls sessionId
 *  on exit, so the session-closed listener can find the tab to auto-remove. */
const remoteSessionTabs = new Map<string, string>();

/** Delay before auto-removing a remote tab after the backend reports session-closed.
 *  Gives the user time to see "[Process exited]" in the terminal before it vanishes. */
const REMOTE_TAB_AUTOCLOSE_MS = 30_000;
/** Shorter delay for agent-spawned sessions — they finish their task and can be cleaned up faster. */
const AGENT_TAB_AUTOCLOSE_MS = 10_000;

interface McpToastListenerState {
	generation: number;
	unlisten?: () => void;
}

interface McpToastPayload {
	title: string;
	message: string | null;
	level: string;
	sound: string | null;
	origin_repo_path?: string;
	origin_session_id?: string;
	__tuic_origin?: { connection: string; name?: string };
}

const MCP_TOAST_LISTENER_KEY = "__tuic_mcp_toast_listener__";

function replaceMcpToastListener(handler: (event: { payload: McpToastPayload }) => void): void {
	const globalState = globalThis as typeof globalThis & {
		[MCP_TOAST_LISTENER_KEY]?: McpToastListenerState;
	};
	const state = globalState[MCP_TOAST_LISTENER_KEY] ?? (globalState[MCP_TOAST_LISTENER_KEY] = { generation: 0 });
	const generation = ++state.generation;
	state.unlisten?.();
	state.unlisten = undefined;

	listen<McpToastPayload>("mcp-toast", handler)
		.then((unlisten) => {
			if (state.generation !== generation) {
				unlisten();
				return;
			}
			state.unlisten = unlisten;
		})
		.catch((err) => appLogger.error("app", "Failed to register mcp-toast listener", err));
}

interface SessionCreatedPayload {
	session_id: string;
	cwd: string | null;
	agent_type?: string | null;
	display_name?: string | null;
	/** `$TUIC_SESSION` of the agent that spawned this PTY; absent for other tabs. */
	parent_session?: string | null;
	/** Present only on an event mirrored from a remote daemon (`remote_mirror.rs`). */
	__tuic_origin?: unknown;
}

function parseAgentType(value: string | null | undefined): AgentType | null {
	return value && (AGENT_TYPES as readonly string[]).includes(value) ? (value as AgentType) : null;
}

/** Dependencies injected into initApp */
export interface AppInitDeps {
	pty: {
		listActiveSessions: () => Promise<
			Array<{
				session_id: string;
				cwd: string | null;
				display_name?: string | null;
				pty_description?: string | null;
				alias?: string | null;
				tuic_session?: string | null;
				display_name_is_custom?: boolean;
				display_name_from_spawn?: boolean;
				is_remote?: boolean;
				parent_session?: string | null;
				state?: {
					shell_state?: "busy" | "idle";
					agent_state?: "starting" | "working" | "awaiting_input" | "idle" | "completed";
					awaiting_input?: boolean;
					question_confident?: boolean;
					agent_type?: string | null;
					agent_intent?: string | null;
					last_prompt?: string | null;
					background_work?: boolean;
					last_activity_ms?: number;
				} | null;
			}>
		>;
		close: (sessionId: string) => Promise<void>;
	};
	setQuitDialogVisible: (visible: boolean) => void;
	setStatusInfo: (msg: string) => void;
	handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
	refreshAllBranchStats: (scopeRepoPath?: string) => Promise<void> | void;
	getDefaultFontSize: () => number;
	stores: {
		hydrate: () => Promise<void>;
		startPolling: () => void;
		stopPolling: () => void;
		startAutoFetch: () => void;
		startPrNotificationTimer: () => void;
		loadFontFromConfig: () => void;
		refreshDictationConfig: () => Promise<void>;
		startUserActivityListening: () => void;
	};
	applyPlatformClass: () => string;
	onCloseRequested: (handler: (event: { preventDefault: () => void }) => void) => void;
	/** Register a repository by path. Same entry point as the `tuic://open-repo`
	 *  deep link (`gitOps.addRepoByPath`), so registration has one implementation.
	 *  Init NEVER calls this on its own — only the parked-tab toast's button does,
	 *  from a click. addRepoByPath calls setActive(), and stealing the focused repo
	 *  from a background event is exactly what b7e6c360 exists to stop. */
	registerRepo: (path: string) => Promise<void>;
}

/** Collect terminal metadata from all repos/branches for persistence */
/** A prompt is persisted only to remind the user what the tab was doing; the whole
 *  text would bloat repositories.json, which is rewritten every 30s. */
const SAVED_PROMPT_MAX_CHARS = 300;

function collectTerminalSnapshots(): Map<string, Map<string, SavedTerminal[]>> {
	const snapshots = new Map<string, Map<string, SavedTerminal[]>>();

	for (const repoPath of repositoriesStore.getPaths()) {
		const repo = repositoriesStore.get(repoPath);
		if (!repo) continue;

		for (const [branchName, branch] of Object.entries(repo.workspaces)) {
			if (branch.terminals.length === 0) continue;

			const saved: SavedTerminal[] = [];
			for (const termId of branch.terminals) {
				const t = terminalsStore.get(termId);
				if (!t) continue;
				saved.push({
					name: t.name,
					cwd: t.cwd,
					fontSize: t.fontSize,
					agentType: t.agentType,
					agentSessionId: t.agentSessionId ?? null,
					tuicSession: t.tuicSession ?? null,
					agentLaunchCommand: t.agentLaunchCommand ?? null,
					alias: t.alias ?? null,
					agentIntent: t.agentIntent ?? null,
					lastPrompt: t.lastPrompt ? t.lastPrompt.slice(0, SAVED_PROMPT_MAX_CHARS) : null,
					suspended: t.suspended,
				});
			}

			if (saved.length > 0) {
				if (!snapshots.has(repoPath)) {
					snapshots.set(repoPath, new Map());
				}
				const branchMap = snapshots.get(repoPath);
				if (branchMap) branchMap.set(branchName, saved);
			}
		}
	}

	return snapshots;
}

/** Attach a backend PTY session to the best matching repo/branch.
 * Remote sessions may use a cwd below a repo or outside every configured repo,
 * so reconnect must use the same ancestor matching and active-branch fallback
 * as the live session-created path. */
function assignSessionToRepoBranch(
	sessionId: string,
	terminalId: string,
	cwd: string | null,
	registerRepo: AppInitDeps["registerRepo"],
	refreshAllBranchStats: AppInitDeps["refreshAllBranchStats"],
): void {
	const owner = resolveRepoOwner(cwd);

	// Record the resolved owner on the terminal itself, BEFORE any placement. The
	// branch arrays are a display index; this field is the truth, and it is what
	// lets reconcileTerminalOwnership move a wrongly-placed tab home later. `null`
	// means "no registered repo owns this cwd" — an honest unknown, not a guess.
	terminalsStore.setRepoPath(terminalId, owner?.repoPath ?? null);

	if (owner) {
		const branchName = placementWorkspaceFor(owner);
		if (branchName) {
			repositoriesStore.addTerminalToWorkspace(owner.repoPath, branchName, terminalId);
			return;
		}
	}

	// No registered repo owns this cwd, so there is no honest placement to make.
	//
	// This used to borrow a slot from `activeRepoPath`. That made an orchestrated
	// session's home depend on where the user happened to be standing when it
	// arrived: two sessions from the SAME unregistered repo landed under two
	// different repos, and neither was the right one. `repoOwnership.ts` exists to
	// keep `activeRepoPath` out of this answer — the fallback had simply survived
	// one level up, here in the caller.
	//
	// An unowned tab goes to the Global Workspace instead: a repo-independent
	// bucket that needs no placement and already surfaces itself in the sidebar
	// with a count. Stable and elsewhere beats visible and arbitrary. `repoPath`
	// stays null above, so `reconcileTerminalOwnership` still walks the tab home
	// the moment a repo claims its cwd.
	// DEFERRED (2026-09-10) — the sidebar entry reads `hasPromoted()`, which counts
	// only the CURRENT scope, so while `useWorktreeConsolidation` holds the store on
	// a repo scope this tab is parked correctly but its badge is not on screen. It
	// is still reachable (the session exists, reconcile re-homes it on registration)
	// and consolidation is opt-in, so this waits for a real report rather than a
	// speculative change to what the badge counts.
	globalWorkspaceStore.promote(terminalId, MANUAL_SCOPE);

	// Which repo the user would have to register to fix this. Without it the
	// warning named only the symptom.
	const unregisteredRoot = unregisteredRepoRootFor(cwd);
	const registeredRoot =
		unregisteredRoot && repositoriesStore.getPaths().find((path) => sameDir(path, unregisteredRoot));
	if (registeredRoot) {
		// A just-created sibling worktree can arrive before the repo's worktree
		// list does. Refresh the registered repo and move this parked tab home
		// once the new workspace is known; registration cannot help here.
		void Promise.resolve()
			.then(() => refreshAllBranchStats(registeredRoot))
			.then(() => reconcileTerminalOwnership(terminalId))
			.catch((err) => appLogger.warn("app", `Failed to refresh worktrees for ${registeredRoot}`, err));
		return;
	}
	appLogger.warn(
		"app",
		`Session ${sessionId}: cwd "${cwd ?? "(null)"}" is owned by no registered repo${
			unregisteredRoot ? ` — register "${unregisteredRoot}" to give it a home` : ""
		} — parked in the Global Workspace until one claims it`,
	);
	if (unregisteredRoot) {
		// Repeats collapse: `hasVisible` dedups on title+message+level+repoPath, so
		// reconnecting twenty sessions from one unregistered repo raises one toast,
		// not twenty. The repoPath argument is deliberately omitted — passing the
		// active repo scoped the toast to it, so walking to another repo defeated
		// the dedup and raised the same warning again there. It is a statement about
		// a directory, not about a repo.
		//
		// The button closes the loop the message opens: naming the directory still left
		// the user to find it in the sidebar and add it by hand. Registration runs ONLY
		// from this click — the user picked the moment, so the setActive() inside
		// addRepoByPath is a repo switch they asked for, not one a background reconnect
		// imposed (b7e6c360). addRepoByPath ends in reconcileTerminalOwnership(), which
		// is what walks the parked tab home once the repo exists.
		toastsStore.add(
			"Tab parked outside your repos",
			`Nothing claims "${unregisteredRoot}". It is in the Global Workspace — register the repo and the tab moves home by itself.`,
			"warn",
			false,
			{
				label: "Register",
				onClick: () => {
					void registerRepo(unregisteredRoot).catch((err) =>
						appLogger.error("app", `Failed to register "${unregisteredRoot}" from the parked-tab toast`, err),
					);
				},
			},
		);
	}
}

/** App initialization: hydrate stores, reconnect PTY sessions, restore state */
export async function initApp(deps: AppInitDeps) {
	const navigation = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
	appLogger.info(
		"app",
		`WebView document navigation=${navigation?.type ?? "unknown"} documentStart=${performance.timeOrigin}`,
	);
	appLogger.info("app", `initApp called — existing terminals: [${terminalsStore.getIds().join(", ")}]`);
	appLogger.debug("app", "SolidJS App mounted");
	const preInitTerminalIds = terminalsStore.getIds();

	const platform = deps.applyPlatformClass();
	appLogger.debug("app", `Platform detected: ${platform}`);

	// Intercept window close for quit confirmation (Story 057)
	deps.onCloseRequested((event) => {
		if (!settingsStore.state.confirmBeforeQuit) return;
		const activeTerminals = terminalsStore.getIds().filter((id) => terminalsStore.get(id)?.sessionId);
		if (activeTerminals.length > 0) {
			event.preventDefault();
			deps.setQuitDialogVisible(true);
		}
	});

	// Periodic terminal snapshot — ensures savedTerminals is always fresh
	// so app restart recovers terminals even if beforeunload fails.
	const SNAPSHOT_INTERVAL_MS = 30_000;
	const snapshotTimer = setInterval(() => {
		const snapshots = collectTerminalSnapshots();
		if (snapshots.size > 0) {
			repositoriesStore.snapshotTerminals(snapshots);
		}
	}, SNAPSHOT_INTERVAL_MS);

	// Snapshot terminal metadata, flush pending saves, and close PTY sessions on app exit
	window.addEventListener("beforeunload", () => {
		clearInterval(snapshotTimer);
		// Every debounced persist has to land here: the timer dies with the
		// WebView, so a preference toggled inside its window is simply lost.
		activityStore.flushSave();
		uiStore.flushSave();
		paneLayoutStore.flushSave();
		mdTabsStore.saveForReload();

		// 1. Snapshot terminal metadata per repo/branch before closing
		const snapshots = collectTerminalSnapshots();
		if (snapshots.size > 0) {
			repositoriesStore.snapshotTerminals(snapshots);
		}

		// 2. Close PTY sessions — but NOT in Tauri mode during webview reloads
		// (Vite HMR, manual reload). The Rust backend survives the reload and
		// list_active_sessions will re-adopt the surviving sessions on re-init.
		// In Tauri, real quit is handled by the close-requested handler which
		// calls app.exit() — beforeunload during quit is a no-op for PTY cleanup.
		if (!isTauri()) {
			// Browser only closes sessions it created — leave Tauri-created ones alive
			for (const sid of browserCreatedSessions) {
				deps.pty.close(sid).catch(() => {});
			}
		}
	});

	// Hydrate all stores from Rust backend
	try {
		await deps.stores.hydrate();
	} catch (err) {
		appLogger.error("app", "Store hydration failed", err);
		deps.setStatusInfo("Warning: store(s) failed to load");
	}

	// Remote machines, at startup rather than when the Settings panel opens.
	// Hydration used to be owned by RemoteMachinesPanel, so until the user walked
	// into Settings the store was empty and nothing else could tell a live machine
	// from a dead one — including the repos registered on it, whose every
	// operation fails while the sidebar still shows them as ordinary.
	// `hydrate()` is idempotent, so the panel may still call it.
	void remoteConnectionsStore.hydrate();

	// Load themes from Rust backend, then apply immediately — the createEffect
	// in App.tsx fires synchronously before this async onMount completes.
	await loadThemes();
	applyAppTheme(settingsStore.state.theme);
	void listenForThemeChanges();

	// Load .tuic.json local configs for all repos (fire-and-forget, non-blocking)
	for (const repoPath of repositoriesStore.getPaths()) {
		repoSettingsStore.loadLocalConfig(repoPath).catch(() => {});
	}

	// Only the live event presents a toast, so a reconnect can never replay
	// historical notifications. Boot reads nothing: the journal is queried when
	// the dialog opens, for the one project it shows.
	subscribeEvents(
		{
			"progress-recorded": (payload) => progressStore.presentLive(payload as ProgressRecordedPayload),
			"workflow-run-changed": (payload) => workflowRunSignals.accept(payload),
		},
		{
			onResync: () => {
				const project = progressStore.requestedProject();
				if (progressStore.dialogVisible() && project) void progressStore.refreshProject(project);
				workflowRunSignals.resync();
			},
		},
	).catch((err) => appLogger.error("app", "Failed to register progress-recorded listener", err));
	if (isTauri()) {
		void listenForNativeNoticeClicks()
			.then((unlisten) => window.addEventListener("beforeunload", unlisten, { once: true }))
			.catch((err) => appLogger.error("app", "Failed to register native notification click listener", err));
	}

	// Recover log entries from Rust backend (survives webview reloads)
	appLogger.hydrateFromRust().catch(() => {});

	// Restore pane layout from disk (terminal tabs will be re-linked during terminal restore)
	await paneLayoutStore.loadFromDisk();

	// Remove splash screen now that stores are hydrated — prevents flash of empty
	// state (e.g. "Add Repository" button) before persisted repos have loaded.
	document.getElementById("splash")?.remove();

	// Repo watchers are started by the Rust setup closure (instant with raw notify).
	// No frontend invoke needed — avoids IPC contention during hydration.

	listen<{ repo_path: string; branch: string }>("head-changed", (event) => {
		const { repo_path, branch } = event.payload;
		const repo = repositoriesStore.get(repo_path);
		if (!repo) return;

		// Only update if branch actually changed
		if (repo.activeWorkspaceId === branch) return;

		appLogger.info("app", `HeadWatcher: ${repo_path} branch changed to ${branch}`);

		const oldBranch = repo.activeWorkspaceId;
		const oldBranchState = oldBranch ? repo.workspaces[oldBranch] : null;

		const isMainCheckout =
			oldBranch &&
			oldBranchState &&
			(oldBranchState.worktreePath === null || oldBranchState.worktreePath === repo_path);

		if (isMainCheckout) {
			// Keyed by `branch` on purpose: HEAD moved, and a row created from a
			// branch has that branch as its id — this is the identity migration's
			// minting rule, the same one `workspace_id_of_worktree` applies in Rust.
			// The lookups below therefore ask "is a row already keyed by this
			// branch", which is exactly the question a rename has to answer.
			// Main checkout (not a worktree): rename the single branch entry so
			// terminals, savedTerminals, hadTerminals etc. carry over seamlessly.
			if (!repo.workspaces[branch]) {
				// Happy path: new branch doesn't exist yet — simple rename.
				repositoriesStore.renameBranch(repo_path, oldBranch, branch);
			} else {
				// Race: refreshAllBranchStats already created the new branch entry.
				// Merge terminal state from old → new, then remove the old entry.
				repositoriesStore.mergeWorkspaceState(repo_path, oldBranch, branch);
				repositoriesStore.removeWorkspace(repo_path, oldBranch);
				repositoriesStore.setActiveWorkspace(repo_path, branch);
			}
		} else {
			// Worktree branch — just ensure target exists and activate it.
			if (!repo.workspaces[branch]) {
				repositoriesStore.setWorkspace(repo_path, branch, { branchName: branch });
			}
			repositoriesStore.setActiveWorkspace(repo_path, branch);
		}

		// Invalidate caches for this repo so next poll fetches fresh data
		invoke("clear_repo_caches", { path: repo_path }).catch((err) =>
			appLogger.debug("app", "Failed to clear repo caches", err),
		);
		// New branch may have a different PR — refresh GitHub status
		githubStore.pollRepo(repo_path);
	}).catch((err) => appLogger.error("app", "Failed to register head-changed listener", err));

	// Listen for .git/ directory changes (index, refs, etc.) to refresh panels.
	// Debounce + in-flight tracking are keyed PER REPO: a `repo-changed` event
	// names the single repo that changed, so we refresh only that repo instead
	// of re-scanning every open repo in unison (which slowed the whole system
	// as the repo count grew). Per-repo keying also means each repo's fresh
	// stats land as soon as that repo finishes — results arrive incrementally,
	// not gated on the slowest repo in a batch.
	const branchStatsTimers = new Map<string, ReturnType<typeof setTimeout>>();
	// Track the in-flight refresh per repo so we can extend that repo's debounce
	// window when another change for it arrives mid-run. FSEvents often fires a
	// burst (worktree delete hits both .git/worktrees/ and the removed dir), and
	// back-to-back refreshes would double-close terminals and thrash store
	// subscriptions. Extended debounce + the refreshGeneration guard collapse the
	// burst into a single run per repo without forcing a UI reset.
	const activeRefreshes = new Map<string, Promise<void>>();
	// Coalesce revision bumps to at most one per repo per animation frame. A real
	// change can still arrive as a same-frame burst (index + refs, or several
	// repos), and each synchronous bump fires the full ~20-effect SolidJS flush.
	// The coalescer collapses the burst WITHOUT losing bumps (each repo is flushed
	// next frame), so panels re-fetch exactly once. (Backend already skips emits
	// when git-state is unchanged; this is defense-in-depth for residual bursts.)
	const revisionCoalescer = createRevisionCoalescer((repoPath, isGitState) =>
		isGitState ? repositoriesStore.bumpGitRevision(repoPath) : repositoriesStore.bumpRevision(repoPath),
	);
	listen<{ repo_path: string; kind: RepoChangeKind }>("repo-changed", (event) => {
		const { repo_path, kind } = event.payload;
		// No cache invalidation here: every backend producer of this event calls
		// `invalidate_repo_caches` before sending it, and `clear_repo_caches` does
		// nothing more — the round trip only ever re-cleared empty caches.
		// Reload .tuic.json (may have changed)
		repoSettingsStore.loadLocalConfig(repo_path).catch(() => {});
		// Signal panels to re-fetch on every logical change, coalesced per frame.
		// (Not folded into the branchStatsTimer below — that setTimeout is cleared
		// on each event, which would drop bumps and leave panels stale, story 1277-31a0.)
		// The kind decides how far the bump reaches: a working-tree change moves
		// only the general revision, so panels reading committed history sit still.
		revisionCoalescer.bump(repo_path, kind === "git-state");
		// Discover external worktree changes for THIS repo only. Use 500ms when
		// idle, 1000ms when this repo's refresh is already running so the next
		// scoped run doesn't race it. Only the branch-stats refresh is debounced;
		// the revision bump above is not. At most one refresh runs per repo, so
		// concurrency is bounded by the number of repos changing in the window —
		// the common single-repo case does one refresh, not N.
		const delay = activeRefreshes.has(repo_path) ? 1000 : 500;
		const existingTimer = branchStatsTimers.get(repo_path);
		if (existingTimer) clearTimeout(existingTimer);
		branchStatsTimers.set(
			repo_path,
			setTimeout(() => {
				branchStatsTimers.delete(repo_path);
				const result = deps.refreshAllBranchStats(repo_path);
				if (result && typeof (result as Promise<void>).then === "function") {
					const inflight = (result as Promise<void>).finally(() => {
						// Only clear if this is still the tracked run (a newer one may have replaced it).
						if (activeRefreshes.get(repo_path) === inflight) activeRefreshes.delete(repo_path);
					});
					activeRefreshes.set(repo_path, inflight);
				}
			}, delay),
		);
	}).catch((err) => appLogger.error("app", "Failed to register repo-changed listener", err));

	// Listen for MCP toast notifications from the Rust backend
	// The native navigation guard cannot tell a real iframe click from a script.
	// Give users a visible fallback while keeping the actual browser open behind
	// the tab menu's explicit action. Browser mode has no Tauri navigation guard.
	if (isTauri()) {
		void listen<string>("navigation-blocked", () => {
			toastsStore.add(
				"External link blocked",
				"This embedded page cannot open external links here. Use Open in Browser from its tab menu.",
				"warn",
				false,
				undefined,
				10000,
				undefined,
				undefined,
				false,
			);
		}).catch((err) => appLogger.error("app", "Failed to register navigation guard listener", err));
	}

	replaceMcpToastListener((event) => {
		const { title, message, level, sound, origin_repo_path, origin_session_id, __tuic_origin: origin } = event.payload;
		if (origin && (typeof title !== "string" || (message !== null && typeof message !== "string"))) {
			appLogger.debug("app", "Discarding malformed mirrored MCP toast");
			return;
		}
		const safeLevel = level === "warn" || level === "error" ? level : "info";
		// Backend notifications stay in the bell; they never cover the active input.
		// Only a registered repo may scope a bell item.
		const terminalId = origin_session_id ? terminalsStore.findBySessionId(origin_session_id) : undefined;
		const repoPath = origin
			? ((terminalId && getSessionConnection(origin_session_id) === origin.connection
					? repositoriesStore.getRepoPathForTerminal(terminalId)
					: undefined) ?? undefined)
			: (resolveRepoForCwd(origin_repo_path) ?? undefined);
		const visibleTitle = origin ? `[${origin.name ?? origin.connection}] ${title}` : title;
		const visibleMessage = message ?? "";
		const action = origin_session_id
			? {
					label: "Open terminal",
					onClick: () => {
						const id = terminalsStore.findBySessionId(origin_session_id);
						if (id && (!origin || getSessionConnection(origin_session_id) === origin.connection))
							navigateToTerminal(id);
					},
				}
			: undefined;
		const noticeId = toastsStore.addToBell(
			visibleTitle,
			visibleMessage,
			safeLevel,
			repoPath,
			action,
			origin_session_id,
			origin?.connection,
		);
		if (noticeId !== -1 && isNotificationSound(sound)) void notificationsStore.play(sound);
	});

	// Listen for sessions created/closed by remote clients (browser UI or other Tauri windows)
	listen<SessionCreatedPayload>("session-created", (event) => {
		const { session_id, cwd, agent_type, display_name, parent_session } = event.payload;
		const parsedAgentType = parseAgentType(agent_type);
		// A mirrored event describes a session on ANOTHER machine. It is stamped
		// `__tuic_origin` by `remote_mirror.rs`; building a tab for it attaches the
		// local transport to a PTY this machine does not run. The mirrored session
		// is already visible as a session-list row carrying its connection id.
		// (The desktop window never hears this name from a mirror at all; the SSE
		// transport carries the whole stream, so the guard lives here too.)
		if (event.payload.__tuic_origin !== undefined) return;
		// Skip if this session was created by the local browser client or is already tracked
		if (browserCreatedSessions.has(session_id)) return;
		const existing = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === session_id);
		if (existing) return;

		appLogger.info("app", `Remote session created: ${session_id}`);
		// `activeId` is null whenever the user is looking at a non-terminal tab, not
		// only when no terminal exists: terminals.ts registers a pane deactivator that
		// clears it. Read the count instead, before the add — reading `activeId` below
		// would pull the user off the panel they opened and onto a worker tab.
		const hadNoTerminals = terminalsStore.getCount() === 0;
		const id = terminalsStore.add({
			sessionId: session_id,
			fontSize: deps.getDefaultFontSize(),
			name:
				display_name ||
				(parsedAgentType
					? `Session ${terminalsStore.getCount() + 1}`
					: `PTY: Session ${terminalsStore.getCount() + 1}`),
			// A spawn-assigned display name is the base title, not a manual rename:
			// an intent title may refine it and a user rename replaces it, but the
			// agent's own OSC title (Claude's session title) must not.
			nameIsCustom: false,
			nameFromSpawn: Boolean(display_name),
			cwd: cwd ?? null,
			awaitingInput: null,
			isRemote: true,
			agentType: parsedAgentType,
			ptyDescription: null,
			parentSession: parent_session ?? null,
		});
		remoteSessionTabs.set(session_id, id);

		assignSessionToRepoBranch(session_id, id, cwd, deps.registerRepo, deps.refreshAllBranchStats);

		// Dock agent-spawned tabs so swarm workers show up in the tab strip.
		// Only for agent_type (MCP agent spawn), not for manually created
		// sessions. The tab is docked but never selected: an MCP spawn must
		// not take over the pane the user is working in.
		if (agent_type) {
			toastsStore.addToBell(
				"Agent started",
				display_name || agent_type,
				"info",
				resolveRepoForCwd(cwd ?? "") ?? undefined,
				{
					label: "Open terminal",
					onClick: () => navigateToTerminal(id),
				},
				session_id,
			);
			// In split mode, ensure there is an active group so assignTabToActiveGroup
			// doesn't silently no-op and leave the tab invisible.
			if (paneLayoutStore.isSplit() && !paneLayoutStore.state.activeGroupId) {
				const leafIds = paneLayoutStore.getAllGroupIds();
				if (leafIds.length > 0) {
					paneLayoutStore.setActiveGroup(leafIds[0]);
				}
			}
			assignTabToActiveGroup(id, "terminal", false);
			// Only steal focus when the app had no terminals at all.
			if (hadNoTerminals) {
				terminalsStore.setActive(id);
			}
		}
	}).catch((err) => appLogger.error("app", "Failed to register session-created listener", err));

	listen<{ session_id: string; description?: string | null }>("pty-description-changed", (event) => {
		const termId = terminalsStore.getTerminalForSession(event.payload.session_id);
		if (termId) terminalsStore.setPtyDescription(termId, event.payload.description ?? null);
	}).catch((err) => appLogger.error("app", "Failed to register pty-description listener", err));

	// An MCP rename starts in the backend; the IPC echo that update() sends
	// back does not emit, so this cannot loop.
	listen<{ session_id: string; name: string; is_custom: boolean }>("session-renamed", (event) => {
		const termId = terminalsStore.getTerminalForSession(event.payload.session_id);
		if (termId) terminalsStore.update(termId, { name: event.payload.name, nameIsCustom: event.payload.is_custom });
	}).catch((err) => appLogger.error("app", "Failed to register session-renamed listener", err));

	// `session action=suspend` waits for this tab's verdict. A client without the tab stays
	// silent: another attached client may own it, and the backend times out if none does.
	listen<{ session_id: string; request_id: string; __tuic_origin?: unknown }>("session-suspend-requested", (event) => {
		if (event.payload.__tuic_origin !== undefined) return;
		const termId = terminalsStore.getTerminalForSession(event.payload.session_id);
		if (!termId) return;
		const requestId = event.payload.request_id;
		suspendTerminal(termId)
			.then((outcome) => {
				if (!outcome.ok)
					appLogger.warn("terminal", "MCP suspend refused by the tab", { termId, reason: outcome.reason });
				return rpc("session_suspend_response", {
					requestId,
					ok: outcome.ok,
					reason: outcome.ok ? null : outcome.reason,
				});
			})
			.catch((err) => appLogger.error("terminal", "Failed to answer the MCP suspend request", err));
	}).catch((err) => appLogger.error("app", "Failed to register session-suspend-requested listener", err));

	listen<{ session_id: string; alias: string; __tuic_origin?: unknown }>("term-alias-assigned", (event) => {
		// A mirrored alias names a session on another machine: no tab here ever
		// binds it, so retaining it would only grow the pending-alias map.
		if (event.payload.__tuic_origin !== undefined) return;
		const { session_id, alias } = event.payload;
		// applyAlias is race-safe: it retains the alias if this event beats
		// setSessionId's binding of session_id to a terminal, and applies it
		// the instant that binding is made — see terminals.ts.
		terminalsStore.applyAlias(session_id, alias);
	}).catch((err) => appLogger.error("app", "Failed to register term-alias-assigned listener", err));

	// The tmux compatibility shim's `set-option ... window-style|
	// pane-border-style|pane-active-border-style` (Claude Code's per-teammate
	// `--agent-color`) resolves to this — see `mcp_http::tmux_routes`.
	listen<{ session_id: string; color?: string | null }>("session-accent-color-changed", (event) => {
		const { session_id, color } = event.payload;
		const termId = terminalsStore.getTerminalForSession(session_id);
		if (termId) terminalsStore.update(termId, { accentColor: color ?? null });
	}).catch((err) => appLogger.error("app", "Failed to register session-accent-color-changed listener", err));

	// The tmux compatibility shim's `select-layout tiled`/`main-vertical`
	// resolves to this — arrange the swarm window's teammate sessions into
	// an actual split view. Sidebar tab list is unaffected: each teammate
	// remains its own independent tab, this only changes what's visible in
	// the terminal area's current split arrangement.
	listen<{ session_ids: string[]; layout: string }>("tmux-window-layout-requested", (event) => {
		const { session_ids, layout } = event.payload;
		paneLayoutStore.arrangeSessionsAsLayout(session_ids, layout);
	}).catch((err) => appLogger.error("app", "Failed to register tmux-window-layout-requested listener", err));

	listen<{ session_id: string; standby: boolean }>("session-standby", (event) => {
		const { session_id, standby } = event.payload;
		const termId = terminalsStore.getTerminalForSession(session_id);
		if (termId) terminalsStore.update(termId, { standby });
	}).catch((err) => appLogger.error("app", "Failed to register session-standby listener", err));

	// Listen for UI tab open/update requests from MCP tools
	listen<{
		id: string;
		title: string;
		html: string;
		pinned: boolean;
		url?: string;
		focus?: boolean;
		origin_repo_path?: string;
	}>("ui-tab", (event) => {
		const { id, title, html, pinned, url, focus, origin_repo_path } = event.payload;

		// Intercept tuic:// protocol URLs — handle as commands, not iframe src
		if (url?.startsWith("tuic://")) {
			try {
				const parsed = new URL(url);
				const cmd = parsed.hostname; // "open", "edit", "terminal"
				const filePath = decodeURIComponent(parsed.pathname).replace(/^\//, "");
				if (!filePath && cmd !== "terminal") return;

				const activeRepoPath = repositoriesStore.state.activeRepoPath;
				const callerRepoPath = resolveRepoForCwd(origin_repo_path);
				const fallbackRepoPath = callerRepoPath ?? activeRepoPath;
				// Registered file ownership wins; otherwise keep the tab with the
				// calling session's repo even when another one is visible.
				let repoPath: string | null = null;
				let relPath = filePath;
				if (isAbsolutePath(filePath)) {
					repoPath = resolveRepoPathFor(filePath);
					if (repoPath) relPath = pathStripPrefix(filePath, repoPath)!;
				} else {
					repoPath = fallbackRepoPath ?? null;
				}

				// A focused native file tab must be visible in the tab bar. File tabs
				// are repo-scoped, so opening a file owned by another registered repo
				// without switching context creates a ghost: its content is active but
				// its tab is filtered out by the current repo. Keep background opens in
				// their repo, but move focused opens to their owning repo first.
				const tabRepoPath = repoPath ?? (isAbsolutePath(filePath) ? fallbackRepoPath : null);
				if (focus !== false && tabRepoPath && tabRepoPath !== activeRepoPath) {
					repositoriesStore.setActive(tabRepoPath);
				}

				// A background open must also stay in the background. Activating it
				// produces the same ghost from the other direction: the repo was
				// deliberately not switched, so an active tab in another repo has its
				// own tab button filtered out of the bar.
				const background = focus === false;

				if (cmd === "open" && isImageFile(filePath)) {
					// The preview tab serves images through the asset protocol, in or out
					// of a repo. The editor cannot read them as UTF-8.
					editorTabsStore.closeMcpFile(id);
					mdTabsStore.closeMcpFile(id);
					mdTabsStore.closeUiTab(id);
					if (repoPath) mdTabsStore.addMcpHtmlPreview(id, repoPath, relPath, pinned, background);
					else mdTabsStore.addMcpHtmlPreview(id, fallbackRepoPath ?? "", filePath, pinned, background);
				} else if (cmd === "open" && repoPath) {
					editorTabsStore.closeMcpFile(id);
					mdTabsStore.closeUiTab(id);
					mdTabsStore.addMcpFile(id, repoPath, relPath, pinned, background);
				} else if (cmd === "open" && isAbsolutePath(filePath)) {
					if (classifyFile(filePath) === "markdown") {
						editorTabsStore.closeMcpFile(id);
						mdTabsStore.closeUiTab(id);
						mdTabsStore.addMcpFile(id, fallbackRepoPath ?? "", filePath, pinned, background);
					} else {
						mdTabsStore.closeMcpFile(id);
						mdTabsStore.closeUiTab(id);
						editorTabsStore.addMcpFile(id, fallbackRepoPath ?? "", filePath, undefined, pinned, {
							externalEditable: false,
							background,
						});
					}
				} else if (cmd === "edit") {
					const line = parseInt(parsed.searchParams.get("line") || "0", 10);
					if (repoPath) {
						mdTabsStore.closeMcpFile(id);
						mdTabsStore.closeUiTab(id);
						editorTabsStore.addMcpFile(id, repoPath, relPath, line || undefined, pinned, {
							externalEditable: false,
							background,
						});
					} else if (isAbsolutePath(filePath)) {
						mdTabsStore.closeMcpFile(id);
						mdTabsStore.closeUiTab(id);
						editorTabsStore.addMcpFile(id, fallbackRepoPath ?? "", filePath, line || undefined, pinned, {
							externalEditable: true,
							background,
						});
					} else {
						appLogger.warn("app", `tuic://edit relative path without active repo: ${filePath}`);
					}
				} else {
					appLogger.warn("app", `tuic:// unhandled: cmd=${cmd} path=${filePath} repo=${repoPath}`);
				}
			} catch (err) {
				appLogger.warn("app", `tuic:// URL parse error: ${url}`, err);
			}
			return;
		}

		mdTabsStore.closeMcpFile(id);
		editorTabsStore.closeMcpFile(id);
		mdTabsStore.openUiTab(id, title, html, pinned, url, focus ?? true, origin_repo_path);
	}).catch((err) => appLogger.error("app", "Failed to register ui-tab listener", err));

	// Keep remoteSessionTabs consistent if the user closes a remote tab manually
	// before the backend session-closed event arrives.
	terminalsStore.onRemove((termId) => {
		for (const [sid, tid] of remoteSessionTabs) {
			if (tid === termId) remoteSessionTabs.delete(sid);
		}
	});

	listen<{ session_id: string; agent_type?: string }>("session-closed", (event) => {
		const { session_id, agent_type } = event.payload;
		// An alias retained for a session no tab ever bound (a desktop-created PTY
		// seen from a browser) is dead once the session is.
		terminalsStore.forgetPendingAlias(session_id);
		// Prefer the persistent remoteSessionTabs map: the store's reverse map may
		// have been cleared already by Terminal.tsx resetting sessionId on pty-exit.
		const termId = remoteSessionTabs.get(session_id) ?? terminalsStore.getTerminalForSession(session_id);
		if (!termId) return;

		remoteSessionTabs.delete(session_id);

		// Countdown + auto-remove is only for MCP-spawned (remote) tabs. Locally-created
		// tabs are managed by Terminal.tsx's pty-exit handler — applying the rename
		// here would leave the name stuck forever because the ticker's isRemote
		// guard aborts on the first tick and the setTimeout's isRemote guard skips removal.
		const t0 = terminalsStore.get(termId);
		if (!t0?.isRemote) return;
		// A suspended tab ended its PTY on purpose and must stay, restorable.
		if (isSuspendingOrSuspended(termId)) return;

		const parsedAgentType = parseAgentType(agent_type);
		handleAgentExitCompletion(termId, parsedAgentType != null);
		terminalsStore.update(termId, { shellState: "exited", sessionId: null });

		// Agent-spawned sessions get a shorter grace period — they finish their task
		// and can be cleaned up faster than manually-opened remote sessions.
		const autoCloseMs = agent_type ? AGENT_TAB_AUTOCLOSE_MS : REMOTE_TAB_AUTOCLOSE_MS;

		appLogger.info("app", `Remote session closed: ${session_id} — tab ${termId} auto-close in ${autoCloseMs}ms`);

		// Countdown in the tab name so the user sees when it will vanish
		const baseName = t0?.name ?? termId;
		let remaining = Math.round(autoCloseMs / 1000);
		terminalsStore.update(termId, { name: `${baseName} (${remaining}s)` });
		const ticker = setInterval(() => {
			remaining--;
			const t = terminalsStore.get(termId);
			if (!t?.isRemote || remaining <= 0) {
				clearInterval(ticker);
				return;
			}
			terminalsStore.update(termId, { name: `${baseName} (${remaining}s)` });
		}, 1000);

		setTimeout(() => {
			clearInterval(ticker);
			const t = terminalsStore.get(termId);
			// Only remove if the tab still exists and is still the remote tab for this
			// session (user may have closed it manually or re-used the slot).
			if (t?.isRemote) {
				appLogger.info("app", `Auto-removing remote tab ${termId} for closed session ${session_id}`);
				terminalsStore.remove(termId);
			}
		}, autoCloseMs);
	}).catch((err) => appLogger.error("app", "Failed to register session-closed listener", err));

	// Close HTML tabs whose creator session has exited
	listen<{ tab_ids: string[] }>("close-html-tabs", (event) => {
		for (const pluginId of event.payload.tab_ids) {
			mdTabsStore.closeUiTab(pluginId);
		}
	}).catch((err) => appLogger.error("app", "Failed to register close-html-tabs listener", err));

	// Screenshot capture: MCP ui(action=screenshot) → capture iframe → respond
	listen<{ id: string; request_id: string }>("screenshot-request", async (event) => {
		const { id, request_id } = event.payload;
		try {
			const container = document.querySelector(`[data-plugin-id="${CSS.escape(id)}"]`);
			const iframe = container?.querySelector("iframe") as HTMLIFrameElement | null;
			if (!iframe) {
				await invoke("screenshot_response", { requestId: request_id, data: null });
				return;
			}
			const { captureIframeAsWebp } = await import("../utils/captureIframe");
			const base64 = await captureIframeAsWebp(iframe);
			await invoke("screenshot_response", { requestId: request_id, data: base64 });
		} catch (err) {
			appLogger.error("app", `Screenshot capture failed for panel '${id}'`, err);
			await invoke("screenshot_response", { requestId: request_id, data: null });
		}
	}).catch((err) => appLogger.error("app", "Failed to register screenshot-request listener", err));

	// Check for surviving PTY sessions (persists across Vite HMR reloads)
	const survivingSessionBaseline = new Map<string, { terminalId: string; shellStateRevision: number }>();
	for (const terminalId of terminalsStore.getIds()) {
		const terminal = terminalsStore.get(terminalId);
		const shellStateRevision = terminalsStore.getShellStateRevision(terminalId);
		if (terminal?.sessionId && shellStateRevision !== null) {
			survivingSessionBaseline.set(terminal.sessionId, { terminalId, shellStateRevision });
		}
	}
	let survivingSessions: Awaited<ReturnType<typeof deps.pty.listActiveSessions>> = [];
	try {
		survivingSessions = await deps.pty.listActiveSessions();
	} catch (err) {
		appLogger.warn("app", "Failed to list active sessions (server unreachable or auth failure)", err);
	}

	// Clear only terminal IDs that existed before initialization. A session-created
	// event may have added a valid remote tab while listActiveSessions was pending.
	for (const id of preInitTerminalIds) {
		terminalsStore.remove(id);
	}

	// Re-adopt surviving PTY sessions or start fresh
	if (survivingSessions.length > 0) {
		appLogger.info("app", `PTY reconnect: found ${survivingSessions.length} surviving session(s)`);
		for (const session of survivingSessions) {
			const existingId = terminalsStore.getTerminalForSession(session.session_id);
			const id =
				existingId ??
				terminalsStore.add({
					sessionId: session.session_id,
					fontSize: deps.getDefaultFontSize(),
					name: session.display_name || terminalsStore.nextDefaultName(),
					nameIsCustom: session.display_name_is_custom ?? false,
					ptyDescription: session.pty_description ?? null,
					isRemote: session.is_remote ?? false,
					agentType: parseAgentType(session.state?.agent_type),
					cwd: session.cwd,
					awaitingInput: null,
				});
			// A session-created event can insert this terminal while the surviving-session
			// request is pending. Reconcile its independent lifecycle fields too, but do
			// not let the older shell snapshot overwrite a newer shell-state event.
			const baseline = survivingSessionBaseline.get(session.session_id);
			const currentRevision = terminalsStore.getShellStateRevision(id);
			const canApplySnapshotShell =
				!existingId ||
				(baseline
					? baseline.terminalId === existingId && baseline.shellStateRevision === currentRevision
					: currentRevision === 0);
			terminalsStore.update(id, {
				...(canApplySnapshotShell && session.state?.shell_state ? { shellState: session.state.shell_state } : {}),
				...(session.is_remote !== undefined ? { isRemote: session.is_remote } : {}),
				...(session.display_name_is_custom !== undefined ? { nameIsCustom: session.display_name_is_custom } : {}),
				// Only the row survives a reload, and only the backend knows where the
				// name came from: every OSC/intent title is synced back as non-custom.
				nameFromSpawn: session.display_name_from_spawn === true,
				parentSession: session.parent_session ?? null,
				...(session.tuic_session ? { tuicSession: session.tuic_session } : {}),
				...(session.state?.agent_type !== undefined ? { agentType: parseAgentType(session.state.agent_type) } : {}),
				// The Context bar mounts once intent or prompt is known. Waiting for the
				// lifecycle sync shows it after the terminal has measured, and the
				// transient taller PTY height duplicates the agent's rows in history.
				// The snapshot is complete, as in useAgentPolling: absence retracts.
				agentIntent: session.state?.agent_intent ?? null,
				lastPrompt: session.state?.last_prompt ?? null,
				ptyDescription: session.pty_description ?? null,
				...(session.alias ? { alias: session.alias } : {}),
				agentState: session.state?.agent_state ?? null,
				awaitingInput: session.state?.awaiting_input === true ? "question" : null,
				awaitingInputConfident: session.state?.question_confident === true,
				backgroundWork: session.state?.background_work ?? false,
				lastActivityAt: session.state?.last_activity_ms ?? null,
			});
			if (session.is_remote) remoteSessionTabs.set(session.session_id, id);

			assignSessionToRepoBranch(session.session_id, id, session.cwd, deps.registerRepo, deps.refreshAllBranchStats);
		}
		terminalsStore.setActive(terminalsStore.getIds()[0]);
	}

	// Ensure non-git repos have a shell branch (migration for repos persisted
	// before the shell-branch feature existed, or added via external paths).
	for (const repoPath of repositoriesStore.getPaths()) {
		const repo = repositoriesStore.get(repoPath);
		if (repo && repo.isGitRepo === false && Object.keys(repo.workspaces).length === 0) {
			const shellBranch = "shell";
			repositoriesStore.setWorkspace(repoPath, shellBranch, {
				worktreePath: repoPath,
				isMain: true,
				isShell: true,
			});
			repositoriesStore.setActiveWorkspace(repoPath, shellBranch);
		}
	}

	// Sessions were attached before the shell-branch migration above ran, so a
	// non-git repo had no branch to claim its own terminals and they were parked
	// elsewhere. Ask again now that every repo can answer.
	reconcileTerminalOwnership();

	// Refresh git stats for persisted repos
	deps.refreshAllBranchStats();

	// Start batch PR/CI polling for all repos
	deps.stores.startPolling();

	// Start per-repo auto-fetch timers
	deps.stores.startAutoFetch();

	// Start PR notification focus timer (auto-dismiss after 5 min focused)
	deps.stores.startPrNotificationTimer();

	// Load font preference from Rust config (single source of truth)
	deps.stores.loadFontFromConfig();

	// Load dictation config from disk
	deps.stores.refreshDictationConfig();

	// Start tracking user activity (click/keydown) for PR display timeouts
	deps.stores.startUserActivityListening();

	try {
		// Restore active repo/branch from persisted state
		const repoPaths = repositoriesStore.getPaths();
		if (repoPaths.length > 0) {
			// Use persisted active repo, falling back to first
			const persistedActive = repositoriesStore.state.activeRepoPath;
			const firstPath = persistedActive && repoPaths.includes(persistedActive) ? persistedActive : repoPaths[0];
			const firstRepo = repositoriesStore.get(firstPath);
			repositoriesStore.setActive(firstPath);
			if (firstRepo?.activeWorkspaceId) {
				if (survivingSessions.length > 0) {
					const branch = firstRepo.workspaces[firstRepo.activeWorkspaceId];
					const validTerminals = branch?.terminals.filter((id) => terminalsStore.getIds().includes(id)) || [];
					if (validTerminals.length > 0) {
						const remembered = branch?.lastActiveTerminal;
						const target = remembered && validTerminals.includes(remembered) ? remembered : validTerminals[0];
						appLogger.info(
							"terminal",
							`initApp RESTORE activeTerminal=${target} (remembered=${remembered}, valid=${JSON.stringify(validTerminals)})`,
						);
						terminalsStore.setActive(target);
					} else {
						await deps.handleBranchSelect(firstPath, firstRepo.activeWorkspaceId);
					}
				} else {
					// Eagerly restore terminals when a pane layout was loaded from disk —
					// the layout references terminal IDs that must exist for panes to render.
					// Without this, the split layout shows empty boxes after a fresh start.
					await deps.handleBranchSelect(firstPath, firstRepo.activeWorkspaceId);
				}
			}
		}
	} finally {
		mdTabsStore.restoreAfterReload();
	}

	// Lazy restore: don't create terminals on startup.
	// Terminals are restored when user clicks a branch in the sidebar.
}
