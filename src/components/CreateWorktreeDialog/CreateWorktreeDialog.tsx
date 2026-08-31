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

const DROPDOWN_MAX_HEIGHT = 200;
// Never shrink the popup below this even when space is tight — better to
// slightly overlap the viewport edge than to render an unusably short list.
const DROPDOWN_MIN_HEIGHT = 100;
// Small gap kept between the popup and whichever viewport edge it opens
// toward, so it doesn't render flush against it.
const DROPDOWN_EDGE_MARGIN = 8;

/** Decide whether the base-ref dropdown should open upward or downward, and how
 * tall it may grow, from the trigger's actual position — recomputed every time
 * it opens rather than assumed. The trigger's position varies run to run (the
 * branch-list reorder above it can be taller or shorter depending on how many
 * matches are showing), so a fixed direction clips at *some* window height no
 * matter which one is hardcoded; picking whichever side currently has more
 * room avoids that.
 *
 * Takes plain numbers (not a real `DOMRect`) so it's directly unit-testable —
 * jsdom/happy-dom's `getBoundingClientRect()` always returns zeros, which
 * would make any test exercising the real DOM API meaningless here. */
export function computeDropdownPlacement(
	triggerRect: { top: number; bottom: number },
	viewportHeight: number,
): { upward: boolean; maxHeight: number } {
	const spaceAbove = triggerRect.top;
	const spaceBelow = viewportHeight - triggerRect.bottom;
	const upward = spaceAbove > spaceBelow;
	const available = Math.max(upward ? spaceAbove : spaceBelow, DROPDOWN_MIN_HEIGHT);
	return { upward, maxHeight: Math.min(DROPDOWN_MAX_HEIGHT, available - DROPDOWN_EDGE_MARGIN) };
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
	/** Disabled while the typed name matches an existing branch (nothing to base it from) —
	 * the row stays mounted rather than unmounting, so the dialog doesn't resize as the
	 * typed name crosses in and out of an exact match. */
	disabled?: boolean;
}> = (props) => {
	const [open, setOpen] = createSignal(false);
	const [query, setQuery] = createSignal("");
	// -1 means "no keyboard cursor yet" — mirrors the branch list below: opening the
	// list or typing a query shouldn't visually pre-select a row until the user moves.
	const [selectedIndex, setSelectedIndex] = createSignal(-1);
	// Recomputed for real every time the list opens (see the effect below) —
	// this default is only ever visible for the one reactive flush between
	// `setOpen(true)` and that effect running, never actually painted.
	const [placement, setPlacement] = createSignal({ upward: true, maxHeight: DROPDOWN_MAX_HEIGHT });
	let triggerRef: HTMLButtonElement | undefined;
	let listRef: HTMLDivElement | undefined;
	let searchRef: HTMLInputElement | undefined;
	// Not a signal: consumed by the "reset search on open" effect below, which
	// otherwise unconditionally clears the query to "" on every open — a plain
	// setQuery() call from the trigger's onKeyDown would just get overwritten by
	// that reset. Read-then-clear inside the same effect keeps a single place
	// responsible for the query's value at open time.
	let pendingQuerySeed = "";

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

	// Force-close if the row becomes disabled while open (typed name became an exact
	// existing-branch match while the list was open) — the `<Show>` below already
	// hides the list from the DOM in that case, but without also resetting `open`
	// here, the list would silently reappear on its own the moment the row becomes
	// enabled again (e.g. typing past the exact match), with no new click.
	createEffect(() => {
		if (props.disabled) setOpen(false);
	});

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
		const seed = pendingQuerySeed;
		pendingQuerySeed = "";
		setQuery(seed);
		setSelectedIndex(-1);
		if (triggerRef) {
			setPlacement(computeDropdownPlacement(triggerRef.getBoundingClientRect(), window.innerHeight));
		}
		// setTimeout (not requestAnimationFrame) so this reliably fires after the
		// dialog's own mount-time name-input focus, which is queued the same way —
		// two rAFs and a 0ms timer don't have a guaranteed relative order, but two
		// same-delay timers run in scheduling order.
		const focusTimer = setTimeout(() => {
			searchRef?.focus();
			// Put the caret after the seeded character rather than at position 0, so
			// keyboard-opening the dropdown by typing continues the search instead of
			// inserting ahead of what the user already typed.
			const len = searchRef?.value.length ?? 0;
			searchRef?.setSelectionRange(len, len);
		}, 0);
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

	// `<For>` only invokes this mapping callback once per distinct option (keyed by
	// reference) — it does NOT re-invoke it when filtering merely shifts that same
	// option to a different position in the list. So the index must be read fresh
	// on every use via the accessor `<For>` hands us, not captured once as a plain
	// number: a closure over a snapshot index goes stale the moment typing removes
	// an earlier match, and a click/hover then acts on the wrong (or out-of-bounds)
	// slot in `filteredRefs()` and silently no-ops. `index` here is a lazy accessor
	// (never called until something actually needs the current value), mirroring
	// how the branch list below reads its own `<For>` index — do not eagerly
	// resolve it to a number at the call site, and do not resolve it via a
	// linear `indexOf` scan either (recomputing on every row, every keystroke).
	const renderItem = (option: BaseRefOption, index: () => number) => (
		<div
			data-index={index()}
			data-testid="base-ref-item"
			class={`${s.dropdownItem} ${option.name === props.value ? s.dropdownItemActive : ""} ${
				index() === selectedIndex() ? s.dropdownItemSelected : ""
			}`}
			onClick={() => {
				props.onChange(option.name);
				setOpen(false);
			}}
			onMouseEnter={() => setSelectedIndex(index())}
		>
			{option.name}
			{option.is_default ? ` (${t("createWorktree.default", "default")})` : ""}
		</div>
	);

	return (
		<div class={s.baseRefRow} data-disabled={props.disabled ? "" : undefined}>
			<label>{t("createWorktree.startFrom", "Start from")}</label>
			<div class={s.dropdownWrapper}>
				<button
					ref={triggerRef}
					type="button"
					class={s.dropdownTrigger}
					data-testid="base-ref-trigger"
					disabled={props.disabled}
					onClick={() => setOpen(!open())}
					onKeyDown={(e) => {
						if (props.disabled) return;
						if (!open()) {
							if (e.key === "Enter" || e.key === "ArrowDown") {
								// A native <button>'s Enter activation fires its click as part of
								// THIS keydown's own default action, so preventDefault() here also
								// suppresses that — safe to also call setOpen ourselves as the
								// single source of truth. ArrowDown has no native activation at
								// all, so it needs this branch regardless.
								e.preventDefault();
								setOpen(true);
								return;
							}
							if (e.key === " ") {
								// Space's native <button> activation fires its click on the
								// SEPARATE keyup event, not this one — preventDefault() here does
								// NOT suppress it. Without the onKeyUp handler below, the list
								// would open here and then immediately re-close from that click's
								// onClick={() => setOpen(!open())} toggle.
								e.preventDefault();
								setOpen(true);
								return;
							}
						}
						if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && document.activeElement === triggerRef) {
							// Typing a printable character while the Tab-focused trigger has
							// focus opens the list and starts filtering immediately, instead of
							// the keystroke being silently swallowed (or, previously, leaking to
							// the terminal underneath — see useKeyboardRedirect). Gated on
							// `activeElement` (not `!open()`) so a fast SECOND character — one
							// landing before the search input's deferred setTimeout(0) focus
							// handoff below has actually run, while this trigger still holds
							// focus — keeps accumulating into the query instead of being
							// dropped, matching what the user would see if focus had already
							// moved to the search box.
							e.preventDefault();
							if (open()) {
								setQuery((q) => q + e.key);
							} else {
								pendingQuerySeed = e.key;
								setOpen(true);
							}
						}
					}}
					onKeyUp={(e) => {
						// Suppresses the native click Space fires on keyup — see the keydown
						// comment above for why this can't be done in keydown alone.
						if (e.key === " ") e.preventDefault();
					}}
				>
					<span class={s.dropdownValue}>{props.value}</span>
					<svg class={s.dropdownChevron} width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
						<path d="M4 6l4 4 4-4" />
					</svg>
				</button>
				<Show when={open() && !props.disabled}>
					<div
						ref={listRef}
						class={`${s.dropdownList} ${placement().upward ? s.dropdownListUp : s.dropdownListDown}`}
						style={{ "max-height": `${placement().maxHeight}px` }}
					>
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
								<For each={filteredLocalRefs()}>{(option, i) => renderItem(option, i)}</For>
							</Show>
							<Show when={filteredRemoteRefs().length > 0}>
								<div class={s.dropdownSectionHeader}>{t("createWorktree.remoteBranches", "Remote")}</div>
								<For each={filteredRemoteRefs()}>
									{(option, i) => renderItem(option, () => filteredLocalRefs().length + i())}
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

	// Truncated for display only — shows the tail (the branch-derived directory
	// name), the informative part, rather than the shared worktrees-dir prefix.
	// A fixed character budget (not CSS text-overflow / direction:rtl) is used
	// deliberately: the common "ellipsize at the start" CSS trick reorders the
	// text via the Unicode Bidi Algorithm, which can visually scramble digit
	// runs in an otherwise strong-LTR string — e.g. a path ending in a
	// date/number segment (`.../worktree-2026-08-28`) — even with no RTL
	// characters anywhere in it. Truncating the actual string sidesteps that
	// entirely; text-overflow: ellipsis in the CSS stays only as a backstop.
	const PATH_PREVIEW_MAX_CHARS = 48;
	const pathPreviewDisplay = createMemo(() => {
		const full = pathPreview();
		if (full.length <= PATH_PREVIEW_MAX_CHARS) return full;
		return `…${full.slice(full.length - PATH_PREVIEW_MAX_CHARS + 1)}`;
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

						{/* Directly under the name field it filters, so it reads as related to
						    that field rather than to the unrelated "Start from" row below. */}
						<div class={s.branchList} ref={branchListRef}>
							<Show
								when={filteredBranches().length > 0}
								fallback={
									<div class={s.dropdownEmpty}>{t("createWorktree.noBranchesMatch", "No existing branches match")}</div>
								}
							>
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
							</Show>
						</div>

						<Show when={availableBaseRefs().length > 1}>
							<BaseRefDropdown
								value={baseRef()}
								options={availableBaseRefs()}
								onChange={setBaseRef}
								disabled={isExistingBranch()}
							/>
						</Show>

						{/* Wrapped in a fixed-min-height footer (rather than making each row
						    always-mounted) so the dialog's overall height stays constant as
						    these rows individually appear/disappear/change per keystroke —
						    each row keeps its original conditional-Show semantics. */}
						<div class={s.previewFooter}>
							<Show when={trimmedName()}>
								<div class={s.statusLine}>
									{isExistingBranch()
										? t("createWorktree.statusExisting", "Will check out existing branch into new worktree")
										: t("createWorktree.statusNew", "Will create new branch and worktree")}
								</div>
							</Show>

							<Show when={pathPreview()}>
								<div class={s.pathPreview}>{pathPreviewDisplay()}</div>
							</Show>

							{error() && <p class={`${d.error} ${s.errorLine}`}>{error()}</p>}
						</div>
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
