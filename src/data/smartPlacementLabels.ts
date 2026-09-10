import type { SmartPlacement } from "../stores/promptLibrary";

/** User-facing label and a one-line description of where a placement actually
 *  surfaces, for the Placement checkbox grid in both prompt editors. Typed as
 *  `Record<SmartPlacement, ...>` so adding a member to the `SmartPlacement`
 *  union is a compile error here until it's given a label — that's also what
 *  keeps `ALL_SMART_PLACEMENTS` below honest instead of a second, plain-array
 *  list of the same values that TypeScript can't force to stay in sync. */
export const SMART_PLACEMENT_INFO: Record<SmartPlacement, { label: string; hint: string }> = {
	toolbar: {
		label: "Toolbar menu",
		hint: "Lightning-bolt Smart Prompts dropdown in the main toolbar",
	},
	"git-changes": {
		label: "Git panel — Changes tab",
		hint: "Inline buttons above the changed-files list",
	},
	"git-branches": {
		label: "Git panel — Branches tab",
		hint: "Inline buttons above the branch list, and the branch right-click menu",
	},
	"pr-popover": {
		label: "Pull request view",
		hint: "PR row and PR detail popover",
	},
	"issue-popover": {
		label: "GitHub issue view",
		hint: "Issues panel popover",
	},
	"terminal-context": {
		label: "Terminal right-click menu",
		hint: "Appears when right-clicking inside a terminal",
	},
	"command-palette": {
		label: "Command Palette",
		hint: 'Cmd+P, then the "Prompts" scope chip or typing the prompt name',
	},
	"file-context": {
		label: "File right-click menu",
		hint: "File Browser, Changes tab, and editor tab right-click menus",
	},
};

/** Every placement value, in the order shown in the Placement checkbox grid
 *  (Prompt Library dialog and Settings > Smart Prompts — both editors share
 *  this list so neither can drift out of sync with `SmartPlacement`). Derived
 *  from `SMART_PLACEMENT_INFO` rather than listed separately, so it can never
 *  omit a value the type above already had to cover. */
export const ALL_SMART_PLACEMENTS = Object.keys(SMART_PLACEMENT_INFO) as SmartPlacement[];
