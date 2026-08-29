import { createMemo, createSignal, For, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import s from "./ImportReviewDialog.module.css";

export type ImportReviewStatus = "new" | "conflict";

export interface ImportReviewItem<T> {
	item: T;
	id: string;
	name: string;
	status: ImportReviewStatus;
	/** True when the item runs code or otherwise needs review before it's safe to enable. */
	needsReview: boolean;
}

export interface ImportReviewDialogProps<T> {
	title: string;
	/** Singular noun used in the subtitle and footer ("prompt" / "rule"). */
	noun: string;
	items: ImportReviewItem<T>[];
	/** Notes from parsing the file — sanitized/dropped entries — shown above the list. */
	warnings: string[];
	/** Optional note shown once, above the bulk-select row — used for a caveat that applies to
	 *  the whole import rather than one row. */
	footnote?: string;
	/** Short secondary detail rendered next to the name (e.g. an execution mode or action count). */
	meta?: (item: T) => string | undefined;
	/** Note shown under a CONFLICT row explaining what importing it will do. */
	conflictNote: (item: T) => string;
	/** Warning text shown under a row flagged `needsReview`. */
	reviewWarning: string;
	onImport: (selectedIds: string[]) => void;
	onCancel: () => void;
}

/** Review list shown before an imported file is applied to a library the user can add to —
 *  Smart Prompts and Smart Selection rules both use this. Every incoming item is listed with its
 *  conflict status; the user picks which ones actually get imported before anything touches the
 *  store. Generic over the item type (`T`) so each caller supplies its own domain-specific
 *  labels (`meta`, `conflictNote`, `reviewWarning`) without duplicating the checkbox-list,
 *  bulk-select, or Escape-handling machinery. */
export function ImportReviewDialog<T>(props: ImportReviewDialogProps<T>) {
	const [selected, setSelected] = createSignal<Set<string>>(new Set(props.items.map((c) => c.id)));

	const toggle = (id: string) => {
		setSelected((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	};

	const selectAll = () => setSelected(new Set(props.items.map((c) => c.id)));
	const selectNone = () => setSelected(new Set<string>());
	const selectNewOnly = () => setSelected(new Set(props.items.filter((c) => c.status === "new").map((c) => c.id)));

	const selectedCount = createMemo(() => selected().size);

	const handleImport = () => {
		if (selectedCount() === 0) return;
		props.onImport(props.items.filter((c) => selected().has(c.id)).map((c) => c.id));
	};

	// Escape-to-close is handled centrally (stores/modalStack): registering routes Escape to
	// props.onCancel AND stops it reaching the terminal underneath.
	registerModal(props.onCancel);

	return (
		<div class={d.overlay} onClick={props.onCancel}>
			<div class={`${d.popover} ${s.wide}`} onClick={(e) => e.stopPropagation()}>
				<div class={d.header}>
					<h4>{props.title}</h4>
				</div>
				<div class={d.body}>
					<p class={s.subtitle}>
						{props.items.length} {props.noun}
						{props.items.length === 1 ? "" : "s"} in file — {props.items.filter((c) => c.status === "conflict").length}{" "}
						already exist
					</p>

					<Show when={props.footnote}>
						{(footnote) => (
							<p class={s.footnote} data-testid="import-footnote">
								{footnote()}
							</p>
						)}
					</Show>

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
						<For each={props.items}>
							{(candidate) => {
								const id = candidate.id;
								return (
									<li class={s.row}>
										<label class={s.rowLabel}>
											<input
												type="checkbox"
												data-testid={`import-check-${id}`}
												checked={selected().has(id)}
												onChange={() => toggle(id)}
											/>
											<span class={s.name}>{candidate.name}</span>
											<span class={`${s.badge} ${candidate.status === "conflict" ? s.badgeConflict : s.badgeNew}`}>
												{candidate.status === "conflict" ? "CONFLICT" : "NEW"}
											</span>
											<Show when={props.meta?.(candidate.item)}>{(text) => <span class={s.mode}>{text()}</span>}</Show>
										</label>
										<Show when={candidate.status === "conflict"}>
											<div class={s.note}>{props.conflictNote(candidate.item)}</div>
										</Show>
										<Show when={candidate.needsReview}>
											<div class={s.warning} data-testid={`import-warning-${id}`}>
												{props.reviewWarning}
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
}
