import type { Component } from "solid-js";
import type { SmartSelectionRule } from "../../components/Terminal/smartSelectionTypes";
import type { RuleImportCandidate } from "../../utils/smartSelectionExport";
import { ImportReviewDialog, type ImportReviewItem } from "../ImportReviewDialog/ImportReviewDialog";

export interface RuleImportDialogProps {
	candidates: RuleImportCandidate[];
	/** Notes from parsing the file — sanitized/dropped entries — shown above the list. */
	warnings: string[];
	/** True when the store has no customizations yet (`smartSelectionRules` is `[]`) — importing
	 *  anything will materialize every built-in rule into `config.json`, so the dialog shows a
	 *  one-time footnote explaining that. */
	willMaterializeDefaults: boolean;
	/** True when an id in the file matches a built-in rule — drives the conflict note's wording. */
	isBuiltIn: (id: string) => boolean;
	onImport: (selectedIds: string[]) => void;
	onCancel: () => void;
}

/** Thin adapter over the generic `ImportReviewDialog`, supplying Smart-Selection-specific
 *  labels: an action-count column, the built-in-vs-custom conflict note, and the
 *  "runs commands / sends text" review warning. See `ImportReviewDialog` for the shared
 *  checkbox-list/bulk-select/Escape machinery this and `PromptImportDialog` both build on. */
export const RuleImportDialog: Component<RuleImportDialogProps> = (props) => {
	const items = (): ImportReviewItem<SmartSelectionRule>[] =>
		props.candidates.map((c) => ({
			item: c.rule,
			id: c.rule.id,
			name: c.rule.name,
			status: c.status,
			needsReview: c.needsReview,
		}));

	return (
		<ImportReviewDialog
			title="Import Smart Selection Rules"
			noun="rule"
			items={items()}
			warnings={props.warnings}
			footnote={
				props.willMaterializeDefaults
					? 'Importing saves a copy of all built-in rules to your configuration. "Restore built-in defaults" undoes this.'
					: undefined
			}
			meta={(r) => `${r.actions.length} action${r.actions.length === 1 ? "" : "s"}`}
			conflictNote={(r) =>
				props.isBuiltIn(r.id)
					? 'Replaces your current version. "Restore built-in defaults" remains available afterwards.'
					: "Replaces your current version of this rule."
			}
			reviewWarning="⚠ Runs commands or sends text to the terminal — will import disabled until you review it."
			onImport={props.onImport}
			onCancel={props.onCancel}
		/>
	);
};
