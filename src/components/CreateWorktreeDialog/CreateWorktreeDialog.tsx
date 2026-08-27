import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import type { BaseRefOption } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { registerModal } from "../../stores/modalStack";
import { validateBranchName } from "../RenameBranchDialog/RenameBranchDialog";
import d from "../shared/dialog.module.css";
import s from "./CreateWorktreeDialog.module.css";

/** Options returned when the user confirms worktree creation */
export interface WorktreeCreateOptions {
	branchName: string;
	createBranch: boolean;
	/** Base ref to create the worktree from (branch name or "HEAD") */
	baseRef: string;
}

export interface CreateWorktreeDialogProps {
	visible: boolean;
	suggestedName: string;
	existingBranches: string[];
	/** Branches that already have worktrees (cannot be checked out again) */
	worktreeBranches: string[];
	/** Base directory where worktrees are created */
	worktreesDir: string;
	/** Available base refs for the "Start from" dropdown (first is default) */
	baseRefs?: BaseRefOption[];
	/** Base ref to preselect when the dialog opens — e.g. the last one used
	 * successfully for this repo this session. Falls back to `baseRefs[0]`. */
	defaultBaseRef?: string;
	/** Generate a random branch name */
	onGenerateName?: () => Promise<string>;
	onClose: () => void;
	onCreate: (options: WorktreeCreateOptions) => void | Promise<void>;
}

/** Sanitize a branch name for use as a directory name (replace slashes with dashes) */
function sanitizeForPath(name: string): string {
	return name.replace(/\//g, "-");
}

/** Wraps the first case-insensitive match of `query` inside `text` in a `<mark>`.
 * The filter is a single substring match, so a single highlighted range is enough —
 * mirrors CommandPalette's `renderMatchLine`. */
const HighlightMatch: Component<{ text: string; query: string }> = (props) => {
	const segments = createMemo(() => {
		const q = props.query.trim().toLowerCase();
		if (!q) return null;
		const idx = props.text.toLowerCase().indexOf(q);
		if (idx < 0) return null;
		return {
			before: props.text.slice(0, idx),
			match: props.text.slice(idx, idx + q.length),
			after: props.text.slice(idx + q.length),
		};
	});

	return (
		<Show when={segments()} fallback={props.text}>
			{(seg) => (
				<>
					{seg().before}
					<mark class={s.matchHighlight}>{seg().match}</mark>
					{seg().after}
				</>
			)}
		</Show>
	);
};

/** Custom styled dropdown replacing native <select>, with local/remote grouping,
 * a search box, and full keyboard navigation. */
const BaseRefDropdown: Component<{
	value: string;
	options: BaseRefOption[];
	onChange: (value: string) => void;
}> = (props) => {
	const [open, setOpen] = createSignal(false);
	const [query, setQuery] = createSignal("");
	// -1 means "no keyboard cursor yet" — mirrors the branch list below: opening the
	// list or typing a query shouldn't visually pre-select a row until the user moves.
	const [selectedIndex, setSelectedIndex] = createSignal(-1);
	let triggerRef: HTMLButtonElement | undefined;
	let listRef: HTMLDivElement | undefined;
	let searchRef: HTMLInputElement | undefined;

	// Flat, filtered list in the same local-then-remote order the backend already
	// returns — this is what keyboard navigation walks, so arrows cross the section
	// headers without the grouped render needing to know about indices itself.
	const filteredRefs = createMemo(() => {
		const q = query().trim().toLowerCase();
		if (!q) return props.options;
		return props.options.filter((r) => r.name.toLowerCase().includes(q));
	});
	const filteredLocalRefs = createMemo(() => filteredRefs().filter((r) => r.kind === "local"));
	const filteredRemoteRefs = createMemo(() => filteredRefs().filter((r) => r.kind === "remote"));

	// Close on outside click
	const handleDocClick = (e: MouseEvent) => {
		if (!triggerRef?.contains(e.target as Node) && !listRef?.contains(e.target as Node)) {
			setOpen(false);
		}
	};

	createEffect(() => {
		if (!open()) return;
		document.addEventListener("mousedown", handleDocClick);
		onCleanup(() => document.removeEventListener("mousedown", handleDocClick));
	});

	// Reset search state on open, autofocus the search input, and claim Escape for
	// just this dropdown while it's open — the modal stack is LIFO, so this entry
	// (pushed after the dialog's own) wins first; a second Escape then falls through
	// to close the dialog itself.
	createEffect(() => {
		if (!open()) return;
		setQuery("");
		setSelectedIndex(-1);
		// setTimeout (not requestAnimationFrame) so this reliably fires after the
		// dialog's own mount-time name-input focus, which is queued the same way —
		// two rAFs and a 0ms timer don't have a guaranteed relative order, but two
		// same-delay timers run in scheduling order.
		const focusTimer = setTimeout(() => searchRef?.focus(), 0);
		onCleanup(() => clearTimeout(focusTimer));
		registerModal(() => setOpen(false));
	});

	// Reset the keyboard cursor whenever the query changes
	createEffect(() => {
		query();
		setSelectedIndex(-1);
	});

	// Scroll the highlighted item into view. Can't index listRef.children directly
	// (the search input and section headers are siblings of the items), so look up
	// by data-index instead.
	createEffect(() => {
		const idx = selectedIndex();
		if (!listRef || idx < 0) return;
		listRef.querySelector(`[data-index="${idx}"]`)?.scrollIntoView({ block: "nearest" });
	});

	const commitSelection = (index: number) => {
		const option = filteredRefs()[index];
		if (!option) return;
		props.onChange(option.name);
		setOpen(false);
	};

	const handleSearchKeydown = (e: KeyboardEvent) => {
		switch (e.key) {
			case "ArrowDown":
				e.preventDefault();
				e.stopPropagation();
				setSelectedIndex((i) => Math.min(i + 1, filteredRefs().length - 1));
				break;
			case "ArrowUp":
				e.preventDefault();
				e.stopPropagation();
				setSelectedIndex((i) => Math.max(i - 1, 0));
				break;
			case "Enter":
				// Stop this from also reaching the dialog's document-level Enter
				// handler, which would submit the whole form while picking a ref.
				e.preventDefault();
				e.stopPropagation();
				commitSelection(selectedIndex());
				break;
			case "Tab":
				e.stopPropagation();
				setOpen(false);
				break;
			default:
				break;
		}
	};

	const renderItem = (option: BaseRefOption, index: number) => (
		<div
			data-index={index}
			data-testid="base-ref-item"
			class={`${s.dropdownItem} ${option.name === props.value ? s.dropdownItemActive : ""} ${
				index === selectedIndex() ? s.dropdownItemSelected : ""
			}`}
			onClick={() => commitSelection(index)}
			onMouseEnter={() => setSelectedIndex(index)}
		>
			{option.name}
			{option.is_default ? ` (${t("createWorktree.default", "default")})` : ""}
		</div>
	);

	return (
		<div class={s.baseRefRow}>
			<label>{t("createWorktree.startFrom", "Start from")}</label>
			<div class={s.dropdownWrapper}>
				<button
					ref={triggerRef}
					type="button"
					class={s.dropdownTrigger}
					data-testid="base-ref-trigger"
					onClick={() => setOpen(!open())}
				>
					<span class={s.dropdownValue}>{props.value}</span>
					<svg class={s.dropdownChevron} width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
						<path d="M4 6l4 4 4-4" />
					</svg>
				</button>
				<Show when={open()}>
					<div ref={listRef} class={s.dropdownList}>
						<input
							ref={searchRef}
							type="text"
							class={s.dropdownSearch}
							data-testid="base-ref-search"
							value={query()}
							onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
							onKeyDown={handleSearchKeydown}
							placeholder={t("createWorktree.searchRefs", "Search branches...")}
							autocomplete="off"
							autocorrect="off"
							spellcheck={false}
						/>
						<Show
							when={filteredRefs().length > 0}
							fallback={<div class={s.dropdownEmpty}>{t("createWorktree.noRefsMatch", "No refs match")}</div>}
						>
							<Show when={filteredLocalRefs().length > 0}>
								<div class={s.dropdownSectionHeader}>{t("createWorktree.localBranches", "Local")}</div>
								<For each={filteredLocalRefs()}>{(option, i) => renderItem(option, i())}</For>
							</Show>
							<Show when={filteredRemoteRefs().length > 0}>
								<div class={s.dropdownSectionHeader}>{t("createWorktree.remoteBranches", "Remote")}</div>
								<For each={filteredRemoteRefs()}>
									{(option, i) => renderItem(option, filteredLocalRefs().length + i())}
								</For>
							</Show>
						</Show>
					</div>
				</Show>
			</div>
		</div>
	);
};

export const CreateWorktreeDialog: Component<CreateWorktreeDialogProps> = (props) => {
	const [branchName, setBranchName] = createSignal("");
	const [baseRef, setBaseRef] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);
	const [isCreating, setIsCreating] = createSignal(false);
	// -1 = no keyboard cursor: Enter creates with whatever's typed, same as today.
	// A non-negative index means "highlighted, not yet accepted" — Enter accepts
	// that row (populates the input) rather than submitting; a second Enter then
	// creates, matching the existing click-to-populate mouse behavior.
	const [branchIndex, setBranchIndex] = createSignal(-1);
	let inputRef: HTMLInputElement | undefined;
	let branchListRef: HTMLDivElement | undefined;

	/** Available base refs — first entry is the default */
	const availableBaseRefs = () => props.baseRefs ?? [];

	const trimmedName = () => branchName().trim();

	// Whether the typed name matches an existing local branch
	const isExistingBranch = createMemo(() => props.existingBranches.includes(trimmedName()));

	// Whether the typed name matches a branch that already has a worktree
	const hasWorktree = createMemo(() => props.worktreeBranches.includes(trimmedName()));

	// Filter branches by what the user has typed
	const filteredBranches = createMemo(() => {
		const query = trimmedName().toLowerCase();
		if (!query) return props.existingBranches;
		return props.existingBranches.filter((b) => b.toLowerCase().includes(query));
	});

	// Indices into filteredBranches() that are selectable (i.e. don't already have
	// a worktree) — the keyboard cursor only ever lands on one of these.
	const selectableIndices = createMemo(() =>
		filteredBranches()
			.map((_, i) => i)
			.filter((i) => !props.worktreeBranches.includes(filteredBranches()[i])),
	);

	const moveBranchCursor = (delta: 1 | -1) => {
		const selectable = selectableIndices();
		if (selectable.length === 0) return;
		const pos = selectable.indexOf(branchIndex());
		if (delta > 0) {
			setBranchIndex(selectable[pos < 0 ? 0 : Math.min(pos + 1, selectable.length - 1)]);
		} else if (pos <= 0) {
			setBranchIndex(-1);
		} else {
			setBranchIndex(selectable[pos - 1]);
		}
	};

	// Path preview
	const pathPreview = createMemo(() => {
		const name = trimmedName();
		if (!name || !props.worktreesDir) return "";
		const dir = props.worktreesDir.replace(/\/$/, "");
		return `${dir}/${sanitizeForPath(name)}/`;
	});

	// Reset state when dialog opens
	createEffect(() => {
		if (props.visible) {
			setBranchName("");
			setBaseRef(props.defaultBaseRef || availableBaseRefs()[0]?.name || "");
			setError(null);
			setBranchIndex(-1);
			// Cleared on re-close/unmount so a stale timer can't fire `.focus()` against
			// a detached input after a rapid close-then-reopen (surfaced by tests that
			// toggle `visible` more than once).
			const focusTimer = setTimeout(() => {
				if (inputRef) {
					inputRef.focus();
				}
			}, 0);
			onCleanup(() => clearTimeout(focusTimer));
		}
	});

	// Scroll the keyboard-highlighted branch row into view
	createEffect(() => {
		const idx = branchIndex();
		if (!branchListRef || idx < 0) return;
		branchListRef.querySelector(`[data-index="${idx}"]`)?.scrollIntoView({ block: "nearest" });
	});

	const acceptHighlightedBranch = () => {
		const branch = filteredBranches()[branchIndex()];
		setBranchIndex(-1);
		if (branch) handleBranchClick(branch);
	};

	// Keyboard handling
	createEffect(() => {
		if (!props.visible) return;

		// Escape-to-close is handled centrally (stores/modalStack): registering routes
		// Escape to props.onClose AND stops it reaching the terminal underneath.
		registerModal(props.onClose);

		const handleKeydown = (e: KeyboardEvent) => {
			// Anything inside the base-ref dropdown (trigger or search box) handles its
			// own Enter/Arrow keys — see BaseRefDropdown's handleSearchKeydown. Committing
			// a ref, or opening/closing that list, must not also drive the branch list or
			// submit the dialog. Solid delegates "keydown" through a single document-level
			// listener (see solid-js/web's DelegatedEvents), so a plain e.stopPropagation()
			// inside the dropdown can't reliably preempt this sibling listener on the same
			// node — checking the target's ancestry here is the robust fix.
			if (e.target instanceof Element && e.target.closest(`.${s.dropdownWrapper}`)) return;

			switch (e.key) {
				case "ArrowDown":
					e.preventDefault();
					moveBranchCursor(1);
					break;
				case "ArrowUp":
					e.preventDefault();
					moveBranchCursor(-1);
					break;
				case "Enter":
					e.preventDefault();
					if (branchIndex() >= 0) {
						acceptHighlightedBranch();
					} else {
						handleCreate();
					}
					break;
				default:
					break;
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	const handleCreate = async () => {
		const name = trimmedName();
		if (!name || isCreating()) return;

		// Existing branch that already has a worktree — reject
		if (hasWorktree()) {
			setError(t("createWorktree.alreadyHasWorktree", "Branch already has a worktree"));
			return;
		}

		const options: WorktreeCreateOptions = isExistingBranch()
			? { branchName: name, createBranch: false, baseRef: baseRef() }
			: (() => {
					const validationError = validateBranchName(name);
					if (validationError) {
						setError(validationError);
						return null!;
					}
					return { branchName: name, createBranch: true, baseRef: baseRef() };
				})();
		if (!options) return;

		setIsCreating(true);
		setError(null);
		try {
			await props.onCreate(options);
		} catch (err) {
			setError(String(err));
			setIsCreating(false);
			return;
		}
		setIsCreating(false);
	};

	const handleInputChange = (e: Event) => {
		const value = (e.target as HTMLInputElement).value;
		setBranchName(value);
		setBranchIndex(-1);
		if (error()) setError(null);
	};

	const handleGenerateName = async () => {
		if (!props.onGenerateName) return;
		const name = await props.onGenerateName();
		setBranchName(name);
		if (error()) setError(null);
	};

	const handleBranchClick = (branch: string) => {
		// Don't allow selecting branches that already have worktrees
		if (props.worktreeBranches.includes(branch)) return;
		setBranchName(branch);
		if (error()) setError(null);
	};

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<span class={d.headerIcon}>+</span>
						<h4>{t("createWorktree.title", "New Worktree")}</h4>
					</div>
					<div class={d.body}>
						<div class={s.inputRow}>
							<input
								ref={inputRef}
								type="text"
								value={branchName()}
								onInput={handleInputChange}
								placeholder={t("createWorktree.comboPlaceholder", "Type branch name or select existing...")}
							/>
							<Show when={props.onGenerateName}>
								<button
									class={s.generateBtn}
									onClick={handleGenerateName}
									title={t("createWorktree.generateName", "Generate random name")}
									type="button"
								>
									<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
										<path d="M13 3.5a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-1a.5.5 0 0 1 0-1h.2A4.5 4.5 0 0 0 8 2.05a4.5 4.5 0 0 0-4.5 4.5.5.5 0 0 1-1 0A5.5 5.5 0 0 1 8 1.05a5.5 5.5 0 0 1 5.5 3.37V4a.5.5 0 0 1 .5-.5zM3 12.5a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h1a.5.5 0 0 1 0 1h-.2A4.5 4.5 0 0 0 8 13.95a4.5 4.5 0 0 0 4.5-4.5.5.5 0 0 1 1 0A5.5 5.5 0 0 1 8 14.95a5.5 5.5 0 0 1-5.5-3.37V12a.5.5 0 0 1-.5.5z" />
									</svg>
								</button>
							</Show>
						</div>

						<Show when={availableBaseRefs().length > 1 && !isExistingBranch()}>
							<BaseRefDropdown value={baseRef()} options={availableBaseRefs()} onChange={setBaseRef} />
						</Show>

						<div class={s.branchList} ref={branchListRef}>
							<For each={filteredBranches()}>
								{(branch, index) => {
									const isDisabled = () => props.worktreeBranches.includes(branch);
									return (
										<div
											data-index={index()}
											data-testid="worktree-branch-item"
											class={`${s.branchItem} ${isDisabled() ? s.disabled : ""} ${trimmedName() === branch ? s.selected : ""} ${
												index() === branchIndex() ? s.branchItemHighlighted : ""
											}`}
											onClick={() => handleBranchClick(branch)}
											onMouseEnter={() => setBranchIndex(isDisabled() ? branchIndex() : index())}
										>
											<span>
												<HighlightMatch text={branch} query={trimmedName()} />
											</span>
											<Show when={isDisabled()}>
												<span class={s.worktreeTag}>{t("createWorktree.hasWorktree", "(has worktree)")}</span>
											</Show>
										</div>
									);
								}}
							</For>
						</div>

						<Show when={trimmedName()}>
							<div class={s.statusLine}>
								{isExistingBranch()
									? t("createWorktree.statusExisting", "Will check out existing branch into new worktree")
									: t("createWorktree.statusNew", "Will create new branch and worktree")}
							</div>
						</Show>

						<Show when={pathPreview()}>
							<div class={s.pathPreview}>{pathPreview()}</div>
						</Show>

						{error() && <p class={d.error}>{error()}</p>}
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={props.onClose}>
							{t("createWorktree.cancel", "Cancel")}
						</button>
						<button class={d.primaryBtn} onClick={handleCreate} disabled={!trimmedName() || isCreating()}>
							{isCreating() ? t("createWorktree.creating", "Creating...") : t("createWorktree.create", "Create")}
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default CreateWorktreeDialog;
