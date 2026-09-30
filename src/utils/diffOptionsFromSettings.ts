import { settingsStore } from "../stores/settings";
import type { DiffOptions } from "../types/diffOptions";

/** Builds a `DiffOptions` object from the shared whitespace/case settings —
 *  every diff-fetching call site (Session Diff Review, Branch Diff Scroll,
 *  per-file DiffTab) reads this instead of poking `settingsStore.state`
 *  directly, so the four fields stay in sync if a fifth option is ever added. */
export function diffOptionsFromSettings(): DiffOptions {
	return {
		ignoreLeadingWs: settingsStore.state.diffIgnoreLeadingWhitespace,
		ignoreTrailingWs: settingsStore.state.diffIgnoreTrailingWhitespace,
		ignoreWsAmount: settingsStore.state.diffIgnoreWhitespaceAmount,
		ignoreCase: settingsStore.state.diffIgnoreCase,
	};
}
