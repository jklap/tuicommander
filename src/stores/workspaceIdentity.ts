import type { SavedTerminal } from "../types";

/**
 * Opaque handle for one workspace. Never parse it: the only guarantee is that
 * it is unique within its repository and stable for the workspace's lifetime.
 *
 * Existing records carry their branch name as the id (see
 * `migrateRepoWorkspaces`), so the value *looks* parseable — reading a branch
 * back out of it couples callers to an implementation detail. Read
 * `WorkspaceState.branchName` instead.
 */
export type WorkspaceId = string;

/** How a workspace's files relate to the repository they came from. */
export type WorkspaceKind =
	/** The repository's own checkout. */
	| "main"
	/** A linked git worktree: refs and objects shared with the parent. */
	| "worktree";

export type WorkspaceCommitStatus = "unmerged" | "merged" | "unknown";
export type WorkspaceRemovalSafety = "safe" | "requires_force" | "unknown";

/** Backend-authored Git lifecycle verdict for one exact workspace id. */
export interface WorkspaceLifecycleStatus {
	dirty: boolean | null;
	commitStatus: WorkspaceCommitStatus;
	removalSafety: WorkspaceRemovalSafety;
	error?: string;
}

/** One workspace with its terminals. */
/** Which multi-step git operation a worktree is in the middle of. Mirrors the Rust
 *  `worktree::GitOpKind` enum's kebab-case wire format exactly. */
export type GitOpKind = "rebase" | "merge" | "cherry-pick" | "revert" | "bisect";

export interface WorkspaceState {
	/** Stable and opaque. Equal to the key that holds this record. */
	workspaceId: WorkspaceId;
	/** What is checked out here. Keep it explicit rather than inferring it from the key. */
	branchName: string;
	kind: WorkspaceKind;
	parentRepoPath: string | null;
	isMain: boolean; // true for main/master/develop
	isShell?: boolean; // true for non-git directory shell entries
	isRemoving?: boolean;
	/** Set while the worktree has a rebase/merge/cherry-pick/revert/bisect in progress; which one. */
	/** In-progress git operation shown as a sidebar badge. `null` clears an
	 * existing value through `setWorkspace` — its `definedFields` guard strips
	 * `undefined`-valued keys, so an optional-field write can never clear. */
	gitOp?: GitOpKind | null;
	/** Set while the backend is warming this worktree's git-ignored build
	 * directories in from the parent repo (`worktree-warm-*` events). `null`
	 * clears it through `setWorkspace`, same as `gitOp` above. `current` is the
	 * just-completed directory's relative path (completion order, not candidate
	 * order) — absent until the first progress tick arrives. */
	warmState?: { status: "warming"; copied: number; total: number; current?: string } | null;
	worktreePath: string | null; // Path to worktree directory (null for main branch)
	terminals: string[]; // terminal IDs belonging to this workspace
	hadTerminals: boolean; // true once a terminal has been created — suppresses auto-spawn after close-all
	lastActiveTerminal: string | null; // last active terminal ID when leaving this workspace
	additions: number;
	deletions: number;
	isMerged: boolean; // true when branch is fully merged into the repo's main branch
	/** Derived on refresh; never persisted as user intent or trusted for deletion. */
	lifecycleStatus?: WorkspaceLifecycleStatus;
	lastCommitTs: number | null; // Unix timestamp of last commit on this branch
	runCommand?: string; // Saved run command for this workspace
	/** Persisted terminal metadata for session restore, keyed by the writing
	 *  client's `CLIENT_INSTANCE_ID` (`stores/clientInstance.ts`) so two
	 *  clients' restart-recovery snapshots merge instead of one clobbering
	 *  the other on every 30s save (B.5). A legacy flat array (pre-migration)
	 *  is folded into a `"legacy"` key by `normalizeLoadedRepo`. Read through
	 *  `savedTerminalsFor()`, never directly — a bare `Object.values()` here
	 *  loses the "which client, how stale" information that TTL eviction and
	 *  per-key merge both need. */
	savedTerminalsByClient?: Record<string, { savedAt: number; terminals: SavedTerminal[] }>;
	/** CI auto-heal: when enabled, CI failures trigger automatic agent fix cycles */
	ciAutoHeal?: { enabled: boolean; attempts: number; lastRunId?: number; healing?: boolean };
	/** Whether the terminal tab list is expanded under this workspace row */
	tabsExpanded?: boolean;
}

/** One entry as it may arrive off disk: any field may be absent. */
type StoredWorkspace = Partial<WorkspaceState> & { name?: string };

/** A repo record as it may arrive off disk: pre-migration, post-migration, or empty. */
interface MigratableRepoRecord {
	branches?: Record<string, StoredWorkspace>;
	workspaces?: Record<string, StoredWorkspace>;
	/** Pre-migration name for the pointer below. Held a branch, which was the key. */
	activeBranch?: string | null;
	activeWorkspaceId?: WorkspaceId | null;
}

/**
 * Give one stored entry the identity fields the UI indexes without checking.
 *
 * Every field is filled from the key rather than defended at each read site, and
 * a record already carrying one keeps it. The `branchName` fill is the sharp
 * one: `compareBranches` calls
 * `branchName.localeCompare`, so a missing value throws inside the sidebar's
 * sort memo. Solid turns a throwing memo into an *undefined* one, so the crash
 * surfaces at the reader (`sortedBranches().length`) and names nothing that
 * leads back to the record that caused it.
 */
function repairIdentity(key: WorkspaceId, stored: StoredWorkspace): WorkspaceState {
	const { name, ...rest } = stored;
	// The legacy `name` field held the branch, and so did the key. Prefer the
	// key: it is what every existing lookup in the app already resolved by.
	void name;
	return {
		...(rest as Omit<WorkspaceState, "workspaceId" | "branchName" | "kind" | "parentRepoPath" | "worktreePath">),
		workspaceId: typeof rest.workspaceId === "string" ? rest.workspaceId : key,
		// A skewed write reaches `compareBranches`, which calls `localeCompare` on
		// it inside the sidebar's sort memo — the failure this repair exists for.
		branchName: typeof rest.branchName === "string" ? rest.branchName : key,
		kind: rest.kind ?? (rest.isMain ? "main" : "worktree"),
		parentRepoPath: asPath(rest.parentRepoPath),
		worktreePath: asPath(rest.worktreePath),
	};
}

/**
 * A path field as the UI indexes it: a string, or `null` for "none".
 *
 * The document is untrusted input, and the realistic corruption is not a hostile
 * write but a shape skew: `get_worktree_paths` became `{id: {branch, path}}` in
 * one commit, and a WebView still holding the pre-change module wrote the whole
 * record into `worktreePath` on its next refresh. That value round-trips through
 * every later load, and reaches `joinPath` — which calls `.replace` on it and
 * takes the whole app down with an error naming neither the field nor the repo.
 *
 * `null` is what every reader already handles, so a corrupt value degrades to
 * "this workspace has no separate checkout" and the next refresh writes the real
 * path back. Salvaging `.path` out of the object is deliberately not done: it
 * encodes one historical shape and would hide the next one.
 */
function asPath(value: unknown): string | null {
	return typeof value === "string" ? value : null;
}

/**
 * Bring one repository record onto workspace-keyed storage.
 *
 * The migration is the **identity function**: a record written before workspaces
 * existed keys its entries by branch name, and every one of those keys becomes
 * the workspace id verbatim. Nothing moves, nothing is renamed on disk, and no
 * id is invented for data that already works — so a document that round-trips
 * through an older build and back is still readable.
 *
 * A record already keyed by `workspaces` goes through the same repair rather
 * than straight through. It is not only the legacy shape that arrives
 * incomplete: a build that shipped a partial version of this migration writes a
 * `workspaces` document with fields missing, and skipping the repair for those
 * records means the gap survives every later load.
 *
 * Pure: returns a fresh map and never touches the record it was given. Callers
 * assign the result; `normalizeLoadedRepo` is the single seam that does so, for
 * both the hydrate path and every record adopted from another client.
 */
export function migrateRepoWorkspaces(repo: MigratableRepoRecord): Record<WorkspaceId, WorkspaceState> {
	const stored = repo.workspaces ?? repo.branches;
	if (!stored) return {};

	const workspaces: Record<WorkspaceId, WorkspaceState> = {};
	for (const [key, entry] of Object.entries(stored)) {
		workspaces[key] = repairIdentity(key, structuredClone(entry));
	}
	return workspaces;
}

/**
 * Resolve which workspace a record points at, across the rename of that pointer.
 *
 * `activeBranch` held a branch name back when the branch *was* the key, so the
 * value needs no translation — only the field name moved. An id naming no
 * workspace is dropped rather than carried: another window can remove a worktree
 * between the write and this read, and a dangling pointer indexes `workspaces`
 * to `undefined` on every access instead of failing once, here.
 */
export function migrateActiveWorkspaceId(repo: MigratableRepoRecord): WorkspaceId | null {
	const active = repo.activeWorkspaceId ?? repo.activeBranch ?? null;
	if (active === null) return null;
	const workspaces = repo.workspaces ?? repo.branches;
	return workspaces && active in workspaces ? active : null;
}

/** Branch names carry `/`, spaces and other characters an id should not. */
function sanitizeForId(branchName: string): string {
	return (
		branchName
			.replace(/[^A-Za-z0-9._-]+/g, "-")
			.replace(/^-+|-+$/g, "")
			.slice(0, 60) || "workspace"
	);
}

function randomSuffix(): string {
	const bytes = new Uint8Array(4);
	crypto.getRandomValues(bytes);
	return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Mint an id for a NEW workspace. Existing records keep the branch name they were
 * migrated with — this is only for the second workspace on a branch and beyond.
 *
 * The branch is kept in the id for legibility in logs and paths, but it is a
 * label, not data: `takenIds` is what makes the result unique, and the caller
 * must read `branchName` when it wants the branch.
 */
/** A `savedTerminalsByClient` entry older than this is dropped on load — a
 *  client that hasn't run in a week almost certainly isn't coming back with
 *  the exact same terminal set, and an unbounded map would otherwise keep
 *  growing by one key per distinct browser/install that ever opened this
 *  repo. */
export const SAVED_TERMINALS_TTL_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * The restorable terminal set for a workspace, across every client that has
 * saved one — B.5's fix for the restart-recovery snapshot being a single
 * flat array every open client overwrote every 30s, so whichever client
 * saved last silently discarded every other client's terminal set.
 *
 * Union of every (non-stale) client's own saved array, deduped only on
 * `tuicSession` when present (the one field in `SavedTerminal` that is
 * actually a stable cross-restart identity — two clients that both saved a
 * snapshot containing the same `tuicSession` are describing the same tab,
 * not two different ones). Entries with no `tuicSession` are never deduped
 * against each other; restoring a rare duplicate is a far smaller cost than
 * dropping a tab that turns out not to have been a duplicate.
 */
export function savedTerminalsFor(workspace: WorkspaceState): SavedTerminal[] {
	const byClient = workspace.savedTerminalsByClient;
	if (!byClient) return [];
	const now = Date.now();
	const seenSessions = new Set<string>();
	const result: SavedTerminal[] = [];
	// Newest-saved client first, so when two clients' snapshots do collide on
	// the same `tuicSession`, the fresher one wins.
	const entries = Object.values(byClient)
		.filter((entry) => now - entry.savedAt <= SAVED_TERMINALS_TTL_MS)
		.sort((a, b) => b.savedAt - a.savedAt);
	for (const entry of entries) {
		for (const terminal of entry.terminals) {
			if (terminal.tuicSession) {
				if (seenSessions.has(terminal.tuicSession)) continue;
				seenSessions.add(terminal.tuicSession);
			}
			result.push(terminal);
		}
	}
	return result;
}

export function generateWorkspaceId(branchName: string, takenIds: readonly WorkspaceId[]): WorkspaceId {
	const stem = sanitizeForId(branchName);
	const taken = new Set(takenIds);
	// 32 bits of randomness collides at ~1 in 4 billion, but "unique" here is a
	// correctness property (two workspaces sharing an id lose each other's
	// terminals), so it is checked rather than assumed.
	for (let attempt = 0; attempt < 100; attempt++) {
		const id = `${stem}~${randomSuffix()}`;
		if (!taken.has(id)) return id;
	}
	throw new Error(`could not mint a unique workspace id for "${branchName}" after 100 attempts`);
}
