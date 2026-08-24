/**
 * Which registered repo owns a path.
 *
 * This is the single owner of that question. Before it existed, seven call sites
 * each answered it their own way — `useAppInit` sorted candidates, the worktree
 * coordinator preferred worktrees, `mdTabs` ignored them entirely, and three
 * others fell back to whichever repo happened to have focus. That last habit is
 * the bug: an orchestrated PTY, a plan-file event or a preview arrives at a moment
 * the user's focus has nothing to do with, so "the repo on screen" is a coin toss
 * that then gets frozen into the tab.
 *
 * Hence the shape of this module: a pure function over an explicit repo map. There
 * is no parameter through which `activeRepoPath` could reach it, so no future call
 * site can quietly reintroduce the fallback. When a path resolves to nothing the
 * answer is `null`, and the CALLER has to decide what that means — visibly.
 */

import type { RepositoryState } from "../stores/repositories";
import { pathStartsWith } from "./pathUtils";

export interface RepoOwner {
	repoPath: string;
	/**
	 * The linked worktree branch that owns the path, or `null` when the path was
	 * matched at the repo root.
	 *
	 * `null` is deliberate and is not the same as "the main branch". Whatever is
	 * checked out at the repo root changes under the user's feet, so freezing a
	 * branch name here would be a lie the moment they switch. Callers resolve the
	 * root's branch through `activeBranch` at the time they need it.
	 */
	branchName: string | null;
}

interface Candidate {
	/** Directory depth of the prefix that matched — deepest wins. */
	depth: number;
	repoPath: string;
	branchName: string | null;
}

/** Split a path into its directory segments, ignoring separator flavour and any
 *  trailing slash. Comparing raw character length instead would rank
 *  `/repo/packages/app/` above the identical `/repo/packages/app`. */
function segments(path: string): string[] {
	return path.split(/[\\/]+/).filter(Boolean);
}

/** Do two paths name the same directory, trailing slash and separator aside? */
function sameDir(left: string, right: string): boolean {
	const a = segments(left);
	const b = segments(right);
	return a.length === b.length && a.every((seg, i) => seg === b[i]);
}

/**
 * Resolve `path` against an explicit repo map.
 *
 * Candidates are every repo root that prefixes the path, plus every branch with a
 * linked worktree directory that does. The deepest matching prefix wins. On a tie
 * the REPO ROOT wins: a directory somebody registered as a repo in its own right
 * outranks the same directory reached through a parent repo's worktree list.
 *
 * Matching is at directory boundaries (`pathStartsWith`), which is what keeps a
 * sibling `repo__wt/` from reading as a child of `repo/`.
 */
export function resolveRepoOwnerIn(
	path: string | null | undefined,
	repos: Record<string, RepositoryState>,
): RepoOwner | null {
	if (!path) return null;

	const candidates: Candidate[] = [];
	for (const [repoPath, repo] of Object.entries(repos)) {
		if (pathStartsWith(path, repoPath)) {
			candidates.push({ depth: segments(repoPath).length, repoPath, branchName: null });
		}
		for (const branch of Object.values(repo.branches)) {
			// A main branch records the repo root as its worktree; that case is
			// already covered by the root candidate above, and adding it again would
			// claim a branch name for a checkout that can change at any time.
			const worktree = branch.worktreePath;
			if (!worktree || sameDir(worktree, repoPath)) continue;
			if (pathStartsWith(path, worktree)) {
				candidates.push({ depth: segments(worktree).length, repoPath, branchName: branch.name });
			}
		}
	}

	if (candidates.length === 0) return null;

	candidates.sort(
		(left, right) => right.depth - left.depth || Number(left.branchName !== null) - Number(right.branchName !== null),
	);

	const { repoPath, branchName } = candidates[0];
	return { repoPath, branchName };
}
