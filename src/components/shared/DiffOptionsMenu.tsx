import { type Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { settingsStore } from "../../stores/settings";
import { uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import s from "./DiffOptionsMenu.module.css";

/**
 * Toolbar popover for the diff-comparison settings shared by Session Diff
 * Review, Branch Diff Scroll, and per-file DiffTab. Reads and writes
 * `settingsStore`/`uiStore` directly — there is no local state here, so all
 * three toolbars stay in sync with each other and with the Settings panel's
 * own "Diffs" section (GeneralTab.tsx).
 */
export const DiffOptionsMenu: Component<{ class?: string }> = (props) => {
	const [open, setOpen] = createSignal(false);
	let rootRef: HTMLDivElement | undefined;

	const anyOptionActive = () =>
		settingsStore.state.diffIgnoreLeadingWhitespace ||
		settingsStore.state.diffIgnoreTrailingWhitespace ||
		settingsStore.state.diffIgnoreWhitespaceAmount ||
		settingsStore.state.diffIgnoreCase ||
		uiStore.state.diffSoftWrap;

	createEffect(() => {
		if (!open()) return;

		const handleClickOutside = (e: MouseEvent) => {
			if (rootRef && !rootRef.contains(e.target as Node)) setOpen(false);
		};
		const handleEscape = (e: KeyboardEvent) => {
			if (e.key === "Escape") setOpen(false);
		};

		// Delay attaching past the click that opened the menu, same as Dropdown.tsx.
		let attached = false;
		const rafId = requestAnimationFrame(() => {
			document.addEventListener("click", handleClickOutside);
			attached = true;
		});
		document.addEventListener("keydown", handleEscape);

		onCleanup(() => {
			cancelAnimationFrame(rafId);
			if (attached) document.removeEventListener("click", handleClickOutside);
			document.removeEventListener("keydown", handleEscape);
		});
	});

	return (
		<div class={cx(s.root, props.class)} ref={(el) => (rootRef = el)}>
			<button
				type="button"
				class={cx(s.trigger, anyOptionActive() && s.triggerActive)}
				title={t("diffOptions.title", "Diff options")}
				aria-expanded={open()}
				data-testid="diff-options-trigger"
				onClick={() => setOpen((v) => !v)}
			>
				<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
					<path d="M1 3h6v1H1V3zm10 0h4v1h-4V3zM8 2a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM1 7h3v1H1V7zm7 0h7v1H7v-1zm-1-1a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM1 11h6v1H1v-1zm10 0h4v1h-4v-1zm-2-1a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3z" />
				</svg>
			</button>
			<Show when={open()}>
				<div class={s.panel} role="menu" data-testid="diff-options-panel">
					<label class={s.row}>
						<input
							type="checkbox"
							checked={settingsStore.state.diffIgnoreLeadingWhitespace}
							onChange={(e) => settingsStore.setDiffIgnoreLeadingWhitespace(e.currentTarget.checked)}
						/>
						<span>{t("diffOptions.ignoreLeadingWs", "Ignore leading whitespace")}</span>
					</label>
					<label class={s.row}>
						<input
							type="checkbox"
							checked={settingsStore.state.diffIgnoreTrailingWhitespace}
							onChange={(e) => settingsStore.setDiffIgnoreTrailingWhitespace(e.currentTarget.checked)}
						/>
						<span>{t("diffOptions.ignoreTrailingWs", "Ignore trailing whitespace")}</span>
					</label>
					<label class={s.row}>
						<input
							type="checkbox"
							checked={settingsStore.state.diffIgnoreWhitespaceAmount}
							onChange={(e) => settingsStore.setDiffIgnoreWhitespaceAmount(e.currentTarget.checked)}
						/>
						<span>{t("diffOptions.ignoreWsAmount", "Ignore whitespace amount")}</span>
					</label>
					<label class={s.row}>
						<input
							type="checkbox"
							checked={settingsStore.state.diffIgnoreCase}
							onChange={(e) => settingsStore.setDiffIgnoreCase(e.currentTarget.checked)}
						/>
						<span>{t("diffOptions.ignoreCase", "Ignore case")}</span>
					</label>
					<div class={s.divider} />
					<label class={s.row}>
						<input
							type="checkbox"
							checked={uiStore.state.diffSoftWrap}
							onChange={(e) => uiStore.setDiffSoftWrap(e.currentTarget.checked)}
						/>
						<span>{t("diffOptions.softWrap", "Soft-wrap long lines")}</span>
					</label>
				</div>
			</Show>
		</div>
	);
};
