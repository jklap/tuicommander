import type { SavedTerminal } from "../types";

/**
 * Opaque handle for one workspace. Never parse it: the only guarantee is that
 * it is unique within its repository and stable for the workspace's lifetime.
 *
 * Existing records carry their branch name as the id (see
 * `migrateRepoWorkspaces`), so the value *looks* parseable — reading a branch
 * back out of it works right up until the first COW workspace, and then it
 * silently returns the wrong branch. Read `WorkspaceState.branchName` instead.
 */
export type WorkspaceId = string;

/** How a workspace's files relate to the repository they came from. */
export type WorkspaceKind =
	/** The repository's own checkout. */
	| "main"
	/** A linked git worktree: refs and objects shared with the parent. */
	| "worktree"
	/** A copy-on-write clone: an independent repository. */
	| "cow";

/** One workspace with its terminals. */
export interface WorkspaceState {
	/** Stable and opaque. Equal to the key that holds this record. */
	workspaceId: WorkspaceId;
	/** What is checked out here. NOT a key — two workspaces may share it. */
	branchName: string;
	kind: WorkspaceKind;
	/** For `kind === "cow"`, the repository this was cloned from. */
	parentRepoPath: string | null;
	isMain: boolean; // true for main/master/develop
	isShell?: boolean; // true for non-git directory shell entries
	isPreparing?: boolean; // true while stale worktree is being cleaned up and recreated in background
	isRemoving?: boolean; // true while worktree removal is in progress
	worktreePath: string | null; // Path to worktree directory (null for main branch)
	terminals: string[]; // terminal IDs belonging to this workspace
	hadTerminals: boolean; // true once a terminal has been created — suppresses auto-spawn after close-all
	lastActiveTerminal: string | null; // last active terminal ID when leaving this workspace
	additions: number;
	deletions: number;
	isMerged: boolean; // true when branch is fully merged into the repo's main branch
	lastCommitTs: number | null; // Unix timestamp of last commit on this branch
	runCommand?: string; // Saved run command for this workspace
	savedTerminals?: SavedTerminal[]; // Persisted terminal metadata for session restore
	/** CI auto-heal: when enabled, CI failures trigger automatic agent fix cycles */
	ciAutoHeal?: { enabled: boolean; attempts: number; lastRunId?: number; healing?: boolean };
	/** Whether the terminal tab list is expanded under this workspace row */
	tabsExpanded?: boolean;
}

/** A repo record as it may arrive off disk: pre-migration, post-migration, or empty. */
interface MigratableRepoRecord {
	branches?: Record<string, Partial<WorkspaceState> & { name?: string }>;
	workspaces?: Record<string, WorkspaceState>;
	/** Pre-migration name for the pointer below. Held a branch, which was the key. */
	activeBranch?: string | null;
	activeWorkspaceId?: WorkspaceId | null;
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
 * Pure: returns a fresh map and never touches the record it was given. Callers
 * assign the result; `normalizeLoadedRepo` is the single seam that does so, for
 * both the hydrate path and every record adopted from another client.
 */
export function migrateRepoWorkspaces(repo: MigratableRepoRecord): Record<WorkspaceId, WorkspaceState> {
	if (repo.workspaces) return structuredClone(repo.workspaces);
	if (!repo.branches) return {};

	const workspaces: Record<WorkspaceId, WorkspaceState> = {};
	for (const [key, branch] of Object.entries(repo.branches)) {
		const { name, ...rest } = structuredClone(branch);
		workspaces[key] = {
			...(rest as Omit<WorkspaceState, "workspaceId" | "branchName" | "kind" | "parentRepoPath">),
			workspaceId: key,
			// The legacy `name` field held the branch, and so did the key. Prefer the
			// key: it is what every existing lookup in the app already resolved by.
			branchName: key,
			// Nothing on disk predates COW, so a record is main or it is a worktree.
			kind: branch.isMain ? "main" : "worktree",
			parentRepoPath: null,
		};
		void name;
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
