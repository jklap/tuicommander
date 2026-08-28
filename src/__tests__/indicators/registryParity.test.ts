import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { INDICATORS } from "../../indicators/registry";

/**
 * The whole point of the indicator registry: make it IMPOSSIBLE for the UI
 * Legend, the real components, and global.css's defaults to drift apart the
 * way they did before (see the customization plan's Context section for the
 * specific mismatches this replaced — wrong colors, wrong borders, a whole
 * "Panels" legend section describing colors nothing applies).
 *
 * In the same source-text-parity genre as transport.test.ts and
 * settingsDoc.test.ts: read the real files off disk and regex them, rather
 * than asserting against each other's assumptions.
 */

function readRepoFile(relPath: string): string {
	return readFileSync(join(process.cwd(), relPath), "utf8");
}

/** Strip CSS/JS block comments so a var name mentioned in prose (e.g. this
 *  test's own file, or a "--ind-terminal-* color" comment) never counts as
 *  a real declaration or usage. */
function stripBlockComments(source: string): string {
	return source.replace(/\/\*[\s\S]*?\*\//g, "");
}

function findVarTokens(source: string): Set<string> {
	const stripped = stripBlockComments(source);
	const matches = stripped.match(/--ind-[a-z0-9-]+/g) ?? [];
	return new Set(matches);
}

const GLOBAL_CSS = readRepoFile("src/global.css");
const TAB_BAR_CSS = readRepoFile("src/components/TabBar/TabBar.module.css");
const SIDEBAR_CSS = readRepoFile("src/components/Sidebar/Sidebar.module.css");
const PANE_TREE_CSS = readRepoFile("src/components/PaneTree/PaneTree.css");
const UI_LEGEND_TSX = readRepoFile("src/components/HelpPanel/UiLegend.tsx");
const FLOATING_TERMINAL_TSX = readRepoFile("src/FloatingTerminal.tsx");
const PR_STATE_BADGE_TSX = readRepoFile("src/components/Sidebar/PrStateBadge.tsx");
const CHANGES_TAB_CSS = readRepoFile("src/components/GitPanel/ChangesTab.module.css");

/** Every `--ind-*` var actually declared in global.css's :root block. */
function definedVars(): Set<string> {
	const rootBlockMatch = stripBlockComments(GLOBAL_CSS).match(/:root\s*\{[\s\S]*?\n\}/);
	expect(rootBlockMatch, "could not find :root block in global.css").toBeTruthy();
	const declarations = rootBlockMatch![0].match(/--ind-[a-z0-9-]+(?=\s*:)/g) ?? [];
	return new Set(declarations);
}

/** Every `--ind-*` var the registry expects to exist. */
function registryVars(): Set<string> {
	const vars = new Set<string>();
	for (const entry of INDICATORS) {
		if (entry.colorVar) vars.add(entry.colorVar);
		if (entry.animVar) vars.add(entry.animVar);
	}
	return vars;
}

const CONSUMING_SOURCES: Record<string, string> = {
	"TabBar.module.css": TAB_BAR_CSS,
	"Sidebar.module.css": SIDEBAR_CSS,
	"PaneTree.css": PANE_TREE_CSS,
	"UiLegend.tsx": UI_LEGEND_TSX,
	"FloatingTerminal.tsx": FLOATING_TERMINAL_TSX,
	"ChangesTab.module.css": CHANGES_TAB_CSS,
};

describe("indicator registry ↔ global.css parity", () => {
	it("every registry colorVar/animVar has a :root default in global.css", () => {
		const missing = [...registryVars()].filter((v) => !definedVars().has(v));
		expect(missing).toEqual([]);
	});

	it("every --ind-* :root default in global.css corresponds to a registry entry (no orphan defaults)", () => {
		const extra = [...definedVars()].filter((v) => !registryVars().has(v));
		expect(extra).toEqual([]);
	});
});

describe("indicator registry ↔ consuming files parity", () => {
	it.each(Object.entries(CONSUMING_SOURCES))(
		"every --ind-* var referenced in %s is declared in global.css",
		(_name, source) => {
			const used = findVarTokens(source);
			const missing = [...used].filter((v) => !definedVars().has(v));
			expect(missing).toEqual([]);
		},
	);

	it("every --ind-* var declared in global.css is referenced by at least one consuming file (no dead defaults)", () => {
		const usedAnywhere = new Set<string>();
		for (const source of Object.values(CONSUMING_SOURCES)) {
			for (const v of findVarTokens(source)) usedAnywhere.add(v);
		}
		const unused = [...definedVars()].filter((v) => !usedAnywhere.has(v));
		expect(unused).toEqual([]);
	});
});

describe("PrStateBadge.tsx ↔ registry parity", () => {
	it("every PR_BADGE_CLASSES key has a matching pr.<key> registry entry", () => {
		const mapBody = PR_STATE_BADGE_TSX.match(/PR_BADGE_CLASSES:\s*Record<string,\s*string>\s*=\s*\{([\s\S]*?)\n\};/);
		expect(mapBody, "could not find PR_BADGE_CLASSES map in PrStateBadge.tsx").toBeTruthy();
		const keys = [...mapBody![1].matchAll(/(?:^|\n)\s*(?:"([a-z-]+)"|([a-z-]+)):/g)].map((m) => m[1] ?? m[2]);
		expect(keys.length).toBeGreaterThan(0);

		const registryIds = new Set(INDICATORS.filter((e) => e.group === "prBadge").map((e) => e.id));
		const missing = keys.filter((k) => !registryIds.has(`pr.${k}`));
		expect(missing).toEqual([]);
	});
});

/**
 * Deliberately NOT wired onto the registry (see the customization plan's
 * "Explicitly out of scope" note) — other renderers of the same visual
 * concepts that this refactor registers a CANONICAL renderer for instead of
 * chasing every duplicate. Listed so the gap is a tracked line, not a
 * silent one; shrink this list as duplicates get consolidated, never grow
 * it to paper over a new one.
 */
const KNOWN_UNREGISTERED = [
	"src/components/ui/StatusBadge.tsx (PrBadge/CiBadge) — separate status-bar badge implementation",
	"src/components/WorktreeManager/WorktreeManager.tsx (private PrBadge) — separate, only 3 states",
	"src/components/ui/CiRing.tsx — hardcoded hex, ignores CSS vars entirely",
	"src/mobile/** — deliberately independent stylesheet (STYLE_GUIDE.md)",
	"TabBar.module.css .tab.standby — opacity-only dim, no color/icon/animation dimension to register",
] as const;

describe("indicator registry — coverage canary", () => {
	it("KNOWN_UNREGISTERED is documented and non-empty (tracks what this refactor deliberately left alone)", () => {
		expect(KNOWN_UNREGISTERED.length).toBeGreaterThan(0);
	});

	it("the registry has at least the entries this refactor shipped with (canary against silent shrinkage)", () => {
		expect(INDICATORS.length).toBeGreaterThanOrEqual(39);
	});
});
