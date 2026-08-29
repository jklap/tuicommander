import type { Component } from "solid-js";
import type { ImportCandidate } from "../../utils/promptExport";
import { ImportReviewDialog, type ImportReviewItem } from "../ImportReviewDialog/ImportReviewDialog";

export interface PromptImportDialogProps {
	candidates: ImportCandidate[];
	/** Notes from parsing the file — sanitized/dropped entries — shown above the list. */
	warnings: string[];
	onImport: (selectedIds: string[]) => void;
	onCancel: () => void;
}

/** Thin adapter over the generic `ImportReviewDialog`, supplying Smart-Prompts-specific labels:
 *  the execution-mode column, the built-in-vs-custom conflict note, and the "runs shell commands"
 *  review warning. See `ImportReviewDialog` for the shared checkbox-list/bulk-select/Escape
 *  machinery this and `RuleImportDialog` both build on. */
export const PromptImportDialog: Component<PromptImportDialogProps> = (props) => {
	const items = (): ImportReviewItem<ImportCandidate["prompt"]>[] =>
		props.candidates.map((c) => ({
			item: c.prompt,
			id: c.prompt.id,
			name: c.prompt.name,
			status: c.status,
			needsReview: c.needsReview,
		}));

	return (
		<ImportReviewDialog
			title="Import Smart Prompts"
			noun="prompt"
			items={items()}
			warnings={props.warnings}
			meta={(p) => p.executionMode ?? "inject"}
			conflictNote={(p) =>
				p.builtIn
					? "Replaces your current version. Reset to default remains available afterwards."
					: "Replaces your current version of this prompt."
			}
			reviewWarning="⚠ Runs shell commands — will import disabled until you review it."
			onImport={props.onImport}
			onCancel={props.onCancel}
		/>
	);
};
