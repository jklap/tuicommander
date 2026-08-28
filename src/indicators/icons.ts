/**
 * Curated monochrome icon shapes for indicator customization. Every shape
 * renders via `<IndicatorIcon>` (see `IndicatorIcon.tsx`) with
 * `fill="currentColor"` — never emoji, never an icon library (AGENTS.md,
 * docs/frontend/STYLE_GUIDE.md "No icon library").
 *
 * Phase 1 seeded only the shapes already in use as defaults (`dot`, `star`,
 * `worktreeFork`, `branchArrow`, `shellTerminal` — moved here verbatim from
 * their old inline-SVG homes). Phase 3 adds the rest of the curated picker
 * set: `ring`, `square`, `triangle`, `diamond`, `chevron`, `arc` (spinner),
 * `bell`, `question`, `bang`, `pause`, `clock`, `check`, `cross`.
 */
export type IconId =
	| "dot"
	| "star"
	| "worktreeFork"
	| "branchArrow"
	| "shellTerminal"
	| "ring"
	| "square"
	| "triangle"
	| "diamond"
	| "chevron"
	| "arc"
	| "bell"
	| "question"
	| "bang"
	| "pause"
	| "clock"
	| "check"
	| "cross";

/** One drawable layer. Most icons are a single layer; `clock` and `pause`
 *  need two (a ring + hands, two bars). */
export type IconLayer =
	| { kind: "circle"; r: number }
	| { kind: "ring"; r: number; strokeWidth: number }
	/** A partial-circumference ring, for the spinner glyph — meant to be
	 *  paired with the "spin" animation. */
	| { kind: "arc"; r: number; strokeWidth: number; dashArray: string }
	| { kind: "rect"; x: number; y: number; width: number; height: number; rx?: number }
	| { kind: "fill"; d: string }
	| { kind: "stroke"; d: string; strokeWidth: number };

export type IconDef = IconLayer | { kind: "composite"; layers: readonly IconLayer[] };

export const INDICATOR_ICON_DEFS: Record<IconId, IconDef> = {
	// Filled circle — the tab-bar status dot's current "●" glyph, as real geometry
	// instead of a text character (needed so a per-state icon SWAP is possible;
	// a shared text glyph can only ever change color, not shape).
	dot: { kind: "circle", r: 6 },
	// Lifted verbatim from RepoSection.tsx BranchIcon's "star" case.
	star: {
		kind: "fill",
		d: "M9.2 1.2v4.4L13 3.2a1.3 1.3 0 1 1 1.3 2.3L10.5 8l3.8 2.5a1.3 1.3 0 1 1-1.3 2.3L9.2 10.4v4.4a1.2 1.2 0 0 1-2.4 0v-4.4L3 13a1.3 1.3 0 1 1-1.3-2.3L5.5 8 1.7 5.5A1.3 1.3 0 0 1 3 3.2l3.8 2.4V1.2a1.2 1.2 0 0 1 2.4 0z",
	},
	// Lifted verbatim from RepoSection.tsx BranchIcon's "worktree" case — the only
	// stroke-based (not fill-based) shape among the Phase 1 defaults.
	worktreeFork: {
		kind: "stroke",
		d: "M5 1.5a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm0 10a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm6-4a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM5 5v2.5a2 2 0 0 0 2 2h2.5M5 10.5V8",
		strokeWidth: 1.5,
	},
	// Lifted verbatim from RepoSection.tsx BranchIcon's "default" case (a non-main
	// branch checked out in the main worktree — the git-branch glyph).
	branchArrow: {
		kind: "fill",
		d: "M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z",
	},
	// Lifted verbatim from RepoSection.tsx BranchIcon's "shell" case.
	shellTerminal: {
		kind: "fill",
		d: "M1 3l5 5-5 5h2l5-5-5-5H1zm7 9h7v2H8v-2z",
	},
	ring: { kind: "ring", r: 5, strokeWidth: 1.5 },
	square: { kind: "rect", x: 3, y: 3, width: 10, height: 10, rx: 1 },
	triangle: { kind: "fill", d: "M8 2.3 14.5 13H1.5z" },
	diamond: { kind: "fill", d: "M8 1.2 14.8 8 8 14.8 1.2 8z" },
	chevron: { kind: "stroke", d: "M3.5 6 8 10.5 12.5 6", strokeWidth: 1.6 },
	arc: { kind: "arc", r: 5.5, strokeWidth: 1.5, dashArray: "24 11" },
	bell: {
		kind: "fill",
		d: "M8 2c-.55 0-1 .45-1 1v.3C4.7 3.9 3.2 5.9 3.2 8.3v2.9L2 13.4h12l-1.2-2.2V8.3c0-2.4-1.5-4.4-3.8-5V3c0-.55-.45-1-1-1zM6.2 14c0 1 .8 1.8 1.8 1.8s1.8-.8 1.8-1.8H6.2z",
	},
	question: {
		kind: "fill",
		d: "M5.7 5.6c.2-1.1 1.1-1.9 2.3-1.9 1.3 0 2.4.9 2.4 2.1 0 .9-.5 1.4-1.1 1.8-.5.3-.9.6-.9 1.2v.4a.75.75 0 0 1-1.5 0v-.4c0-1.1.7-1.7 1.3-2.1.5-.3.7-.5.7-.9 0-.5-.5-.9-1-.9-.6 0-1 .3-1.1.9a.75.75 0 1 1-1.4-.4zM8 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2z",
	},
	bang: { kind: "fill", d: "M7.25 3h1.5l-.25 7h-1l-.25-7zM8 12a1 1 0 1 1 0 2 1 1 0 0 1 0-2z" },
	pause: {
		kind: "composite",
		layers: [
			{ kind: "rect", x: 4, y: 3, width: 3, height: 10 },
			{ kind: "rect", x: 9, y: 3, width: 3, height: 10 },
		],
	},
	clock: {
		kind: "composite",
		layers: [
			{ kind: "ring", r: 5.5, strokeWidth: 1.3 },
			{ kind: "stroke", d: "M8 5v3.2l2.2 1.6", strokeWidth: 1.3 },
		],
	},
	check: { kind: "stroke", d: "M3.5 8.5 6.5 11.5 12.5 5", strokeWidth: 1.6 },
	cross: { kind: "stroke", d: "M4 4 12 12M12 4 4 12", strokeWidth: 1.6 },
};

export function isKnownIconId(id: string): id is IconId {
	// Not `id in INDICATOR_ICON_DEFS` — `in` also matches inherited Object.prototype keys
	// ("constructor", "toString", "hasOwnProperty", ...), which would wrongly validate a
	// hand-edited config.json entry like `{"icon": "constructor"}`.
	// Not `Object.hasOwn` either — this project's tsconfig targets ES2021 (no ES2022 lib).
	// biome-ignore lint: Object.hasOwn isn't in the ES2021 lib target this project uses.
	return Object.prototype.hasOwnProperty.call(INDICATOR_ICON_DEFS, id);
}

/** Every curated icon id, in a stable display order for the picker. */
export const ICON_IDS: readonly IconId[] = Object.keys(INDICATOR_ICON_DEFS) as IconId[];
