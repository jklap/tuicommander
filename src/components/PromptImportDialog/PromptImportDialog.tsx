import { type Component, createMemo, createSignal, For, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import type { ImportCandidate } from "../../utils/promptExport";
import d from "../shared/dialog.module.css";
import s from "./PromptImportDialog.module.css";

export interface PromptImportDialogProps {
	candidates: ImportCandidate[];
	/** Notes from parsing the file — sanitized/dropped entries — shown above the list. */
	warnings: string[];
	onImport: (selectedIds: string[]) => void;
	onCancel: () => void;
}

/** Review list shown before an imported Smart Prompts file is applied. Every incoming prompt
 *  is listed with its conflict status; the user picks which ones actually get imported before
 *  anything touches the library. Modeled on PostMergeCleanupDialog's checkbox-list pattern. */
export const PromptImportDialog: Component<PromptImportDialogProps> = (props) => {
	const [selected, setSelected] = createSignal<Set<string>>(new Set(props.candidates.map((c) => c.prompt.id)));

	const toggle = (id: string) => {
		setSelected((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	};

	const selectAll = () => setSelected(new Set(props.candidates.map((c) => c.prompt.id)));
	const selectNone = () => setSelected(new Set<string>());
	const selectNewOnly = () =>
		setSelected(new Set(props.candidates.filter((c) => c.status === "new").map((c) => c.prompt.id)));

	const selectedCount = createMemo(() => selected().size);

	const handleImport = () => {
		if (selectedCount() === 0) return;
		props.onImport(props.candidates.filter((c) => selected().has(c.prompt.id)).map((c) => c.prompt.id));
	};

	// Escape-to-close is handled centrally (stores/modalStack): registering routes Escape to
	// props.onCancel AND stops it reaching the terminal underneath.
	registerModal(props.onCancel);

	return (
		<div class={d.overlay} onClick={props.onCancel}>
			<div class={`${d.popover} ${s.wide}`} onClick={(e) => e.stopPropagation()}>
				<div class={d.header}>
					<h4>Import Smart Prompts</h4>
				</div>
				<div class={d.body}>
					<p class={s.subtitle}>
						{props.candidates.length} prompt{props.candidates.length === 1 ? "" : "s"} in file —{" "}
						{props.candidates.filter((c) => c.status === "conflict").length} already exist
					</p>

					<Show when={props.warnings.length > 0}>
						<div class={s.warningBanner} data-testid="import-parse-warnings">
							<For each={props.warnings}>{(w) => <div>{w}</div>}</For>
						</div>
					</Show>

					<div class={s.bulkRow}>
						<button type="button" class={s.bulkBtn} onClick={selectAll}>
							All
						</button>
						<button type="button" class={s.bulkBtn} onClick={selectNone}>
							None
						</button>
						<button type="button" class={s.bulkBtn} onClick={selectNewOnly}>
							New only
						</button>
					</div>

					<ul class={s.list}>
						<For each={props.candidates}>
							{(candidate) => {
								const id = candidate.prompt.id;
								return (
									<li class={s.row}>
										<label class={s.rowLabel}>
											<input
												type="checkbox"
												data-testid={`import-check-${id}`}
												checked={selected().has(id)}
												onChange={() => toggle(id)}
											/>
											<span class={s.name}>{candidate.prompt.name}</span>
											<span class={`${s.badge} ${candidate.status === "conflict" ? s.badgeConflict : s.badgeNew}`}>
												{candidate.status === "conflict" ? "CONFLICT" : "NEW"}
											</span>
											<span class={s.mode}>{candidate.prompt.executionMode ?? "inject"}</span>
										</label>
										<Show when={candidate.status === "conflict"}>
											<div class={s.note}>
												{candidate.prompt.builtIn
													? "Replaces your current version. Reset to default remains available afterwards."
													: "Replaces your current version of this prompt."}
											</div>
										</Show>
										<Show when={candidate.needsReview}>
											<div class={s.warning} data-testid={`import-warning-${id}`}>
												⚠ Runs shell commands — will import disabled until you review it.
											</div>
										</Show>
									</li>
								);
							}}
						</For>
					</ul>
				</div>
				<div class={d.actions}>
					<button class={d.cancelBtn} data-testid="import-cancel-btn" onClick={props.onCancel}>
						Cancel
					</button>
					<button
						class={d.primaryBtn}
						data-testid="import-confirm-btn"
						disabled={selectedCount() === 0}
						onClick={handleImport}
					>
						Import {selectedCount()}
					</button>
				</div>
			</div>
		</div>
	);
};
