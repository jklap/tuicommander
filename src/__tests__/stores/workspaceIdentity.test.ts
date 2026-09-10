import { describe, expect, it } from "vitest";
import { generateWorkspaceId, migrateActiveWorkspaceId, migrateRepoWorkspaces } from "../../stores/workspaceIdentity";

/**
 * A record in the exact shape `repositories.json` holds today, captured from a
 * live config (35 repos): a main branch with no worktree, plus two worktree
 * branches — one of them with a `/` in its name, which is the case an id
 * generator that builds paths gets wrong.
 *
 * The migration's whole promise is that this document survives it unchanged
 * except for the key rename. If a field disappears here, a user loses tab
 * placement, diffstats or a saved run command on the next start.
 */
function legacyRepoRecord() {
	return {
		path: "/Users/x/Gits/acme",
		displayName: "acme",
		initials: "AC",
		isGitRepo: true,
		expanded: true,
		collapsed: false,
		parked: false,
		activeBranch: "main",
		branches: {
			main: {
				name: "main",
				isMain: true,
				worktreePath: null,
				terminals: [],
				hadTerminals: true,
				lastActiveTerminal: "term-1",
				additions: 0,
				deletions: 0,
				isMerged: false,
				lastCommitTs: 1788000000,
				savedTerminals: [{ id: "term-1", name: "zsh", cwd: "/Users/x/Gits/acme", fontSize: 13, agentType: null }],
				tabsExpanded: true,
			},
			"feat/shared-identity": {
				name: "feat/shared-identity",
				isMain: false,
				worktreePath: "/Users/x/Gits/acme__wt/feat-shared-identity",
				terminals: [],
				hadTerminals: false,
				lastActiveTerminal: null,
				additions: 12,
				deletions: 3,
				isMerged: false,
				lastCommitTs: 1788100000,
				savedTerminals: [],
				runCommand: "pnpm dev",
			},
			"POC-000006": {
				name: "POC-000006",
				isMain: false,
				worktreePath: "/Users/x/Gits/acme__wt/POC-000006",
				terminals: [],
				hadTerminals: false,
				lastActiveTerminal: null,
				additions: 0,
				deletions: 0,
				isMerged: true,
				lastCommitTs: null,
				ciAutoHeal: { enabled: true, attempts: 2 },
			},
		},
	};
}

describe("workspace identity migration", () => {
	it("keys every existing entry by its own branch name — the migration is the identity function", () => {
		const workspaces = migrateRepoWorkspaces(legacyRepoRecord());

		expect(Object.keys(workspaces).sort()).toEqual(["POC-000006", "feat/shared-identity", "main"]);
		for (const [key, workspace] of Object.entries(workspaces)) {
			expect(workspace.workspaceId).toBe(key);
			expect(workspace.branchName).toBe(key);
		}
	});

	it("carries every persisted field across untouched", () => {
		const legacy = legacyRepoRecord();
		const workspaces = migrateRepoWorkspaces(legacy);

		const feature = workspaces["feat/shared-identity"];
		expect(feature.worktreePath).toBe("/Users/x/Gits/acme__wt/feat-shared-identity");
		expect(feature.additions).toBe(12);
		expect(feature.deletions).toBe(3);
		expect(feature.runCommand).toBe("pnpm dev");

		const main = workspaces.main;
		expect(main.savedTerminals).toEqual([
			{ id: "term-1", name: "zsh", cwd: "/Users/x/Gits/acme", fontSize: 13, agentType: null },
		]);
		expect(main.lastActiveTerminal).toBe("term-1");
		expect(main.tabsExpanded).toBe(true);
		expect(main.lastCommitTs).toBe(1788000000);

		expect(workspaces["POC-000006"].ciAutoHeal).toEqual({ enabled: true, attempts: 2 });
		expect(workspaces["POC-000006"].isMerged).toBe(true);
	});

	it("does not mutate the record it was given", () => {
		const legacy = legacyRepoRecord();
		const before = JSON.stringify(legacy);
		migrateRepoWorkspaces(legacy);
		expect(JSON.stringify(legacy)).toBe(before);
	});

	it("derives kind from the existing shape and leaves parentRepoPath null", () => {
		const workspaces = migrateRepoWorkspaces(legacyRepoRecord());

		expect(workspaces.main.kind).toBe("main");
		expect(workspaces["feat/shared-identity"].kind).toBe("worktree");
		expect(workspaces["POC-000006"].kind).toBe("worktree");
		for (const workspace of Object.values(workspaces)) {
			expect(workspace.parentRepoPath).toBeNull();
		}
	});

	it("is idempotent — a record already holding workspaces comes back unchanged", () => {
		const once = migrateRepoWorkspaces(legacyRepoRecord());
		const twice = migrateRepoWorkspaces({ workspaces: once });
		expect(twice).toEqual(once);
	});

	it("returns an empty map for a repo that has no entries at all", () => {
		expect(migrateRepoWorkspaces({})).toEqual({});
		expect(migrateRepoWorkspaces({ branches: {} })).toEqual({});
	});

	it("lets two workspaces on one branch coexist under different ids", () => {
		const workspaces = migrateRepoWorkspaces(legacyRepoRecord());
		const second = generateWorkspaceId("feat/shared-identity", Object.keys(workspaces));

		const both = {
			...workspaces,
			[second]: { ...workspaces["feat/shared-identity"], workspaceId: second, kind: "cow" as const },
		};

		expect(Object.keys(both)).toHaveLength(4);
		const sameBranch = Object.values(both).filter((w) => w.branchName === "feat/shared-identity");
		expect(sameBranch).toHaveLength(2);
		expect(new Set(sameBranch.map((w) => w.workspaceId)).size).toBe(2);
	});
});

describe("migrateActiveWorkspaceId", () => {
	it("carries the legacy activeBranch across as the active id", () => {
		expect(migrateActiveWorkspaceId(legacyRepoRecord())).toBe("main");
	});

	it("prefers an already-migrated activeWorkspaceId over the legacy field", () => {
		// A record another client already migrated must not be dragged backwards by
		// a stale `activeBranch` the older build left behind next to it.
		const record = { ...legacyRepoRecord(), activeWorkspaceId: "feat/shared-identity" };
		expect(migrateActiveWorkspaceId(record)).toBe("feat/shared-identity");
	});

	it("is null when neither field is set", () => {
		expect(migrateActiveWorkspaceId({})).toBeNull();
	});

	it("drops an active id that names no workspace", () => {
		// A branch removed in another window leaves the pointer dangling; keeping it
		// would index `workspaces` to undefined on every read.
		const record = { ...legacyRepoRecord(), activeBranch: "deleted-elsewhere" };
		expect(migrateActiveWorkspaceId(record)).toBeNull();
	});
});

describe("generateWorkspaceId", () => {
	it("produces a different id every time for the same branch", () => {
		const ids = new Set(Array.from({ length: 50 }, () => generateWorkspaceId("feat/x", [])));
		expect(ids.size).toBe(50);
	});

	it("sanitizes a branch name that would otherwise read as a path", () => {
		const id = generateWorkspaceId("feat/shared-identity", []);
		expect(id).not.toContain("/");
		expect(id).toMatch(/^feat-shared-identity~[0-9a-f]{8}$/);
	});

	it("never collides with an id already in use", () => {
		// Force the generator to notice a taken id rather than trusting randomness.
		const taken: string[] = [];
		for (let i = 0; i < 20; i++) taken.push(generateWorkspaceId("dup", taken));
		expect(new Set(taken).size).toBe(20);
	});

	it("keeps a branch name that is already safe intact ahead of the suffix", () => {
		expect(generateWorkspaceId("main", [])).toMatch(/^main~[0-9a-f]{8}$/);
	});
});
