import { AGENT_TYPES, type AgentType } from "../agents";
import { handleAgentExitCompletion } from "../components/Terminal/agentExitCompletion";
import { t } from "../i18n";
import { invoke, listen } from "../invoke";
import { isNotificationSound } from "../notifications";
import { activityStore } from "../stores/activityStore";
import { appLogger } from "../stores/appLogger";
import { conversationStore } from "../stores/conversationStore";
import { editorTabsStore } from "../stores/editorTabs";
import { githubStore } from "../stores/github";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../stores/globalWorkspace";
import { mdTabsStore, resolveRepoForCwd } from "../stores/mdTabs";
import { notificationsStore } from "../stores/notifications";
import { paneLayoutStore } from "../stores/paneLayout";
import { type ProgressRecordedPayload, progressStore } from "../stores/progress";
import { repoSettingsStore } from "../stores/repoSettings";
import { placementWorkspaceFor, repositoriesStore, resolveRepoOwner, resolveRepoPathFor } from "../stores/repositories";
import type { WorkspaceState } from "../stores/workspaceIdentity";
import { settingsStore } from "../stores/settings";
import { reconcileTerminalOwnership } from "../stores/terminalOwnership";
import { terminalsStore } from "../stores/terminals";
import { toastsStore } from "../stores/toasts";
import { uiStore } from "../stores/ui";
import { applyAppTheme, listenForThemeChanges, loadThemes } from "../themes";
import { subscribeEvents } from "../transport";
import type { RepoChangeKind, SavedTerminal } from "../types";
import { assignTabToActiveGroup } from "../utils/paneTabAssign";
import { isAbsolutePath, pathStripPrefix } from "../utils/pathUtils";
import { unregisteredRepoRootFor } from "../utils/repoOwnership";
import { createRevisionCoalescer } from "./revisionCoalescer";

/** PTY sessions created by THIS client (desktop or browser). Since B.4 removed
 *  the `beforeunload` auto-close, this set's only remaining purpose is to
 *  drop the create-echo for a session this client is itself mid-creating —
 *  never "sessions we might kill" (nothing in this file kills a session on
 *  unload anymore, on either transport). */
export const locallyCreatedSessions = new Set<string>();

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
				display_name_is_custom?: boolean;
				is_remote?: boolean;
				alias?: string | null;
				state?: {
					shell_state?: "busy" | "idle";
					agent_state?: "starting" | "working" | "awaiting_input" | "idle" | "completed";
					awaiting_input?: boolean;
					question_confident?: boolean;
					agent_type?: string | null;
					background_work?: boolean;
					declared_background_work?: boolean;
				} | null;
			}>
		>;
		close: (sessionId: string) => Promise<void>;
	};
	setQuitDialogVisible: (visible: boolean) => void;
	setStatusInfo: (msg: string) => void;
	setCurrentRepoPath: (path: string | undefined) => void;
	setCurrentBranch: (branch: string | null) => void;
	handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
	refreshAllBranchStats: (scopeRepoPath?: string) => Promise<void> | void;
	handleWorktreeSetupScriptCompleted: (payload: {
		repoPath: string;
		branch: string;
		worktreePath: string;
		exitCode: number | null;
		error: string | null;
	}) => void;
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

		// Snapshot terminal metadata per repo/branch before closing.
		//
		// This handler used to also close every PTY session this browser
		// client created (`for (const sid of ...) deps.pty.close(sid)`).
		// Removed (B.4): `beforeunload` fires on a plain page refresh (F5)
		// exactly like a real tab close, and `deps.pty.close` is a REAL kill
		// of the shared backend PTY process — there is no such thing as a
		// client-local "detach." A browser-created session is byte-identical
		// to a desktop-created one once it exists (`is_remote` is the only
		// structural difference), and persistent PTYs surviving a client
		// disconnect is the explicit point of the remote/PWA feature. On
		// reload the client re-adopts live sessions via `listActiveSessions`
		// (see the init path below) rather than re-spawning, so nothing
		// leaks by leaving this out — and the deliberate "close this tab"
		// user action still kills the session on both transports via its own
		// explicit call to `deps.pty.close`.
		//
		// Rejected alternatives: closing only "empty/unused" sessions
		// (unknowable client-side — a fresh agent tab that hasn't printed
		// yet is exactly the one that must not be killed); an
		// `ephemeral: true` opt-in flag (new wire surface for a behavior
		// nobody asked for, needs backend liveness tracking that doesn't
		// exist); a user-facing setting (a footgun either default is wrong
		// for someone).
		const snapshots = collectTerminalSnapshots();
		if (snapshots.size > 0) {
			repositoriesStore.snapshotTerminals(snapshots);
		}
	});

	// Hydrate all stores from Rust backend
	try {
		await deps.stores.hydrate();
	} catch (err) {
		appLogger.error("app", "Store hydration failed", err);
		deps.setStatusInfo("Warning: store(s) failed to load");
	}

	// Load themes from Rust backend, then apply immediately — the createEffect
	// in App.tsx fires synchronously before this async onMount completes.
	await loadThemes();
	applyAppTheme(settingsStore.state.theme);
	void listenForThemeChanges();

	// Load .tuic.json local configs for all repos (fire-and-forget, non-blocking)
	for (const repoPath of repositoriesStore.getPaths()) {
		repoSettingsStore.loadLocalConfig(repoPath).catch(() => {});
	}

	// Authoritative reads recover state; only the live event presents a toast,
	// so refresh and reconnect can never replay historical notifications.
	void progressStore.refreshAll();
	subscribeEvents(
		{ "progress-recorded": (payload) => progressStore.presentLive(payload as ProgressRecordedPayload) },
		{ onResync: () => void progressStore.refreshAll(progressStore.panelVisible()) },
	).catch((err) => appLogger.error("app", "Failed to register progress-recorded listener", err));

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

	// Background copy of ignored/untracked/explicit-listed files into a freshly
	// created worktree (see `worktree_sync.rs` / `worktree::spawn_worktree_file_sync`).
	// Only fires when the repo actually has something configured to copy.
	listen<{ repoPath: string; branch: string }>("worktree-sync-started", (event) => {
		const { repoPath, branch } = event.payload;
		toastsStore.add(
			t("worktreeSync.started.title", "Syncing files into {branch}…", { branch }),
			"",
			"info",
			false,
			undefined,
			undefined,
			repoPath,
		);
	}).catch((err) => appLogger.error("app", "Failed to register worktree-sync-started listener", err));

	listen<{ repoPath: string; branch: string; copied: number; total: number; errors: string[] }>(
		"worktree-sync-completed",
		(event) => {
			const { repoPath, branch, copied, total, errors } = event.payload;
			const message =
				errors.length > 0
					? t("worktreeSync.completed.withSkips", "Synced {copied} of {total} files ({skipped} skipped)", {
							copied: String(copied),
							total: String(total),
							skipped: String(errors.length),
						})
					: t("worktreeSync.completed.clean", "Synced {copied} file(s)", { copied: String(copied) });
			toastsStore.add(
				t("worktreeSync.completed.title", "Finished syncing {branch}", { branch }),
				message,
				"info",
				false,
				undefined,
				undefined,
				repoPath,
			);
		},
	).catch((err) => appLogger.error("app", "Failed to register worktree-sync-completed listener", err));

	// The setup script (if configured) runs after the sync above, in the same
	// background chain (see `worktree::spawn_worktree_setup_chain`) — its
	// outcome arrives here instead of a synchronous response from worktree
	// creation.
	listen<{
		repoPath: string;
		branch: string;
		worktreePath: string;
		exitCode: number | null;
		error: string | null;
	}>("worktree-setup-script-completed", (event) => {
		deps.handleWorktreeSetupScriptCompleted(event.payload);
	}).catch((err) => appLogger.error("app", "Failed to register worktree-setup-script-completed listener", err));

	// Background warming of git-ignored build directories (see
	// `cow::warm_worktree` / `worktree::run_worktree_warm`), now the FIRST stage
	// of the same background chain the sync/setup-script events above report
	// on. Drives the sidebar's per-row "Warming…" badge (`RepoSection.tsx`)
	// rather than a toast — this can run for tens of seconds on a large repo,
	// so a live, per-row progress indicator is more useful than a one-shot
	// notification. Silent (no events at all) when there's nothing to warm or
	// the repo has warming disabled — mirrors worktree-sync-started's
	// nothing-to-do silence.
	//
	// `updateWarmState` only WRITES to a workspace row that already exists —
	// `setWorkspace` silently fabricates a new row (with `worktreePath: null`
	// and other wrong defaults) when the key is missing, which the `worktree-created`
	// event's own handler is the one meant to do with the real data. A warm-*
	// event racing ahead of that (both are dual-emitted independently, so
	// delivery order isn't guaranteed) must never win that race and plant a
	// phantom/incorrectly-defaulted sidebar row of its own.
	const updateWarmState = (repoPath: string, branch: string, warmState: WorkspaceState["warmState"]) => {
		if (!repositoriesStore.get(repoPath)?.workspaces[branch]) return;
		repositoriesStore.setWorkspace(repoPath, branch, { warmState });
	};

	listen<{ repoPath: string; branch: string; total: number }>("worktree-warm-started", (event) => {
		const { repoPath, branch, total } = event.payload;
		updateWarmState(repoPath, branch, { status: "warming", copied: 0, total });
	}).catch((err) => appLogger.error("app", "Failed to register worktree-warm-started listener", err));

	listen<{ repoPath: string; branch: string; copied: number; total: number; current: string | null }>(
		"worktree-warm-progress",
		(event) => {
			const { repoPath, branch, copied, total, current } = event.payload;
			updateWarmState(repoPath, branch, { status: "warming", copied, total, current: current ?? undefined });
		},
	).catch((err) => appLogger.error("app", "Failed to register worktree-warm-progress listener", err));

	listen<{ repoPath: string; branch: string; warmed: number; warnings: string[] }>(
		"worktree-warm-completed",
		(event) => {
			const { repoPath, branch } = event.payload;
			updateWarmState(repoPath, branch, null);
		},
	).catch((err) => appLogger.error("app", "Failed to register worktree-warm-completed listener", err));

	// Listen for MCP toast notifications from the Rust backend
	replaceMcpToastListener((event) => {
		const { title, message, level, sound, origin_repo_path, origin_session_id } = event.payload;
		const safeLevel = level === "warn" || level === "error" ? level : "info";
		// The repo is not glued into the message any more — the toast renders it as
		// its own badge, so an unregistered origin still names its repo and a
		// registered one does not say it twice.
		// Still only a REGISTERED repo: this field scopes the toast (and the bell
		// item mirrored from it), so an unregistered cwd must not become a repo key.
		const repoPath = resolveRepoForCwd(origin_repo_path) ?? undefined;
		const visibleMessage = message ?? "";
		const duplicate = toastsStore.hasVisible(title, visibleMessage, safeLevel, repoPath);
		// repoPath is already undefined without an origin, and the session id is
		// independent of it — an agent can be bound to a PTY whose cwd resolves to
		// no registered repo — so both ride along on one call.
		toastsStore.add(title, visibleMessage, safeLevel, false, undefined, undefined, repoPath, origin_session_id);
		if (!duplicate && isNotificationSound(sound)) void notificationsStore.play(sound);
	});

	// Listen for sessions created/closed by remote clients (browser UI or other Tauri windows)
	listen<{ session_id: string; cwd: string | null; agent_type?: string | null; display_name?: string | null }>(
		"session-created",
		(event) => {
			const { session_id, cwd, agent_type, display_name } = event.payload;
			const parsedAgentType = parseAgentType(agent_type);
			// Skip if this session was created by this client itself (either
			// transport — see `locallyCreatedSessions`'s doc comment) or is
			// already tracked.
			if (locallyCreatedSessions.has(session_id)) return;
			const existing = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === session_id);
			if (existing) return;

			appLogger.info("app", `Remote session created: ${session_id}`);
			const id = terminalsStore.add({
				sessionId: session_id,
				fontSize: deps.getDefaultFontSize(),
				name:
					display_name ||
					(parsedAgentType
						? `Session ${terminalsStore.getCount() + 1}`
						: `PTY: Session ${terminalsStore.getCount() + 1}`),
				// A spawn-assigned display name is the base title, not a manual rename.
				// Intent/OSC titles may replace it until the user explicitly renames the tab.
				nameIsCustom: false,
				cwd: cwd ?? null,
				awaitingInput: null,
				isRemote: true,
				agentType: parsedAgentType,
				ptyDescription: null,
			});
			remoteSessionTabs.set(session_id, id);

			assignSessionToRepoBranch(session_id, id, cwd, deps.registerRepo);

			// Dock agent-spawned tabs so swarm workers show up in the tab strip.
			// Only for agent_type (MCP agent spawn), not for manually created
			// sessions. The tab is docked but never selected: an MCP spawn must
			// not take over the pane the user is working in.
			if (agent_type) {
				// In split mode, ensure there is an active group so assignTabToActiveGroup
				// doesn't silently no-op and leave the tab invisible.
				if (paneLayoutStore.isSplit() && !paneLayoutStore.state.activeGroupId) {
					const leafIds = paneLayoutStore.getAllGroupIds();
					if (leafIds.length > 0) {
						paneLayoutStore.setActiveGroup(leafIds[0]);
					}
				}
				assignTabToActiveGroup(id, "terminal", false);
				// Only steal focus when there is no existing active terminal.
				if (!terminalsStore.state.activeId) {
					terminalsStore.setActive(id);
				}
			}
		},
	).catch((err) => appLogger.error("app", "Failed to register session-created listener", err));

	listen<{ session_id: string; description?: string | null }>("pty-description-changed", (event) => {
		const termId = terminalsStore.getTerminalForSession(event.payload.session_id);
		if (termId) terminalsStore.setPtyDescription(termId, event.payload.description ?? null);
	}).catch((err) => appLogger.error("app", "Failed to register pty-description listener", err));

	listen<{ session_id: string; alias: string }>("term-alias-assigned", (event) => {
		const { session_id, alias } = event.payload;
		// applyAlias is race-safe: it retains the alias if this event beats
		// setSessionId's binding of session_id to a terminal, and applies it
		// the instant that binding is made — see terminals.ts.
		terminalsStore.applyAlias(session_id, alias);
	}).catch((err) => appLogger.error("app", "Failed to register term-alias-assigned listener", err));

	// `PUT /sessions/:id/name` (and its Tauri-command twin) previously mutated
	// display_name with no emit at all, so a rename was invisible until the next
	// full app restart — the tab title was only ever set from session-created's
	// payload or the init-time GET /sessions read, neither of which re-fires
	// later. This listener is the fix: the tmux shim's `select-pane -T` (and any
	// other future caller of the rename route) now actually updates the tab.
	listen<{ session_id: string; display_name?: string | null; is_custom: boolean }>("session-renamed", (event) => {
		const { session_id, display_name, is_custom } = event.payload;
		const termId = terminalsStore.getTerminalForSession(session_id);
		if (!termId) return;
		// A `null` display_name means "no name set", not "set to empty string" —
		// coercing it to `""` here used to echo an empty name straight back to
		// the backend (terminalsStore.update()'s own echo guard fires on any
		// `name` key), destroying that "no name set" state for good. Only
		// include `name` when there's a real value; always update nameIsCustom.
		terminalsStore.update(termId, {
			...(display_name != null ? { name: display_name } : {}),
			nameIsCustom: is_custom,
		});
	}).catch((err) => appLogger.error("app", "Failed to register session-renamed listener", err));

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

	// A hardware controller (StreamDock macropad, etc.) asking the UI to
	// focus a session's tab — see `AppEvent::SessionFocusRequested`'s doc
	// comment (state.rs). Must set BOTH stores: `executeSmartPrompt` reads
	// `terminalsStore.getActive()`, but the conversation engine keys on the
	// active conversation — see `watcherFire.ts`'s own `setActiveSession`
	// for the same requirement.
	listen<{ session_id: string }>("session-focus-requested", (event) => {
		const termId = terminalsStore.getTerminalForSession(event.payload.session_id);
		if (termId) {
			terminalsStore.setActive(termId);
			conversationStore.setActiveTerminal(termId);
		}
	}).catch((err) => appLogger.error("app", "Failed to register session-focus-requested listener", err));

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
				// Resolve: absolute path → the repo that owns it, relative → active repo
				// (a relative path typed into a tuic:// link means "here", so focus IS
				// the right answer for that case and only that case).
				let repoPath: string | null = null;
				let relPath = filePath;
				if (isAbsolutePath(filePath)) {
					repoPath = resolveRepoPathFor(filePath);
					if (repoPath) relPath = pathStripPrefix(filePath, repoPath)!;
				} else {
					repoPath = activeRepoPath ?? null;
				}

				// A focused native file tab must be visible in the tab bar. File tabs
				// are repo-scoped, so opening a file owned by another registered repo
				// without switching context creates a ghost: its content is active but
				// its tab is filtered out by the current repo. Keep background opens in
				// their repo, but move focused opens to their owning repo first.
				if (focus !== false && repoPath && repoPath !== activeRepoPath) {
					const repo = repositoriesStore.get(repoPath);
					repositoriesStore.setActive(repoPath);
					deps.setCurrentRepoPath(repoPath);
					deps.setCurrentBranch(repo?.activeWorkspaceId ?? null);
				}

				// A background open must also stay in the background. Activating it
				// produces the same ghost from the other direction: the repo was
				// deliberately not switched, so an active tab in another repo has its
				// own tab button filtered out of the bar.
				const background = focus === false;

				if (cmd === "open" && repoPath) {
					if (background) mdTabsStore.addFileBackground(repoPath, relPath);
					else mdTabsStore.add(repoPath, relPath);
				} else if (cmd === "open" && isAbsolutePath(filePath)) {
					editorTabsStore.add("__external__", filePath, undefined, { externalEditable: false, background });
				} else if (cmd === "edit") {
					const line = parseInt(parsed.searchParams.get("line") || "0", 10);
					if (repoPath) {
						editorTabsStore.add(repoPath, relPath, line || undefined, { background });
					} else if (isAbsolutePath(filePath)) {
						editorTabsStore.add("__external__", filePath, line || undefined, {
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

		const parsedAgentType = parseAgentType(agent_type);
		handleAgentExitCompletion(termId, parsedAgentType != null);
		terminalsStore.update(termId, { shellState: "exited", sessionId: null });

		// Agent-spawned sessions get a shorter grace period — they finish their task
		// and can be cleaned up faster than manually-opened remote sessions. Keyed
		// off the PARSED type, not the raw field's truthiness — an unrecognized
		// agent name string must not accidentally pick the short timer.
		const autoCloseMs = parsedAgentType != null ? AGENT_TAB_AUTOCLOSE_MS : REMOTE_TAB_AUTOCLOSE_MS;

		appLogger.info("app", `Remote session closed: ${session_id} — tab ${termId} auto-close in ${autoCloseMs}ms`);

		// Countdown in the tab name so the user sees when it will vanish. `{
		// echo: false }`: this is cosmetic-only display text, never the
		// session's real display name — previously this relied on `sessionId`
		// already being nulled above to implicitly suppress the echo, which
		// broke silently if that write's ordering ever changed.
		const baseName = t0?.name ?? termId;
		let remaining = Math.round(autoCloseMs / 1000);
		terminalsStore.update(termId, { name: `${baseName} (${remaining}s)` }, { echo: false });
		const ticker = setInterval(() => {
			remaining--;
			const t = terminalsStore.get(termId);
			if (!t?.isRemote || remaining <= 0) {
				clearInterval(ticker);
				return;
			}
			terminalsStore.update(termId, { name: `${baseName} (${remaining}s)` }, { echo: false });
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
					alias: session.alias ?? null,
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
				...(session.state?.agent_type !== undefined ? { agentType: parseAgentType(session.state.agent_type) } : {}),
				...(session.alias !== undefined ? { alias: session.alias ?? null } : {}),
				ptyDescription: session.pty_description ?? null,
				agentState: session.state?.agent_state ?? null,
				awaitingInput: session.state?.awaiting_input === true ? "question" : null,
				awaitingInputConfident: session.state?.question_confident === true,
				backgroundWork: session.state?.background_work ?? false,
				declaredBackgroundWork: session.state?.declared_background_work ?? false,
			});
			if (session.is_remote) remoteSessionTabs.set(session.session_id, id);

			assignSessionToRepoBranch(session.session_id, id, session.cwd, deps.registerRepo);
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

	// Restore active repo/branch from persisted state
	const repoPaths = repositoriesStore.getPaths();
	if (repoPaths.length > 0) {
		// Use persisted active repo, falling back to first
		const persistedActive = repositoriesStore.state.activeRepoPath;
		const firstPath = persistedActive && repoPaths.includes(persistedActive) ? persistedActive : repoPaths[0];
		const firstRepo = repositoriesStore.get(firstPath);
		repositoriesStore.setActive(firstPath);
		deps.setCurrentRepoPath(firstPath);
		if (firstRepo?.activeWorkspaceId) {
			deps.setCurrentBranch(firstRepo.activeWorkspaceId);
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
			return;
		}
	}

	// Lazy restore: don't create terminals on startup.
	// Terminals are restored when user clicks a branch in the sidebar.
}
