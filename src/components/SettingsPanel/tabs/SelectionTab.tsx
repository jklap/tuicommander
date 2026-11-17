import { type Component, createMemo, createSignal, For, Index, Show } from "solid-js";
import { DEFAULT_WORD_SEPARATORS, settingsStore } from "../../../stores/settings";
import { toastsStore } from "../../../stores/toasts";
import { onClickKeyDown } from "../../../utils/a11y";
import { isValidRegex } from "../../../utils/isValidRegex";
import { exportJsonWithToast, pickJsonImportFile } from "../../../utils/jsonFileTransfer";
import type { ExportScope } from "../../../utils/promptExport";
import { randomId } from "../../../utils/randomId";
import {
	buildRulesExportFile,
	classifyRuleImport,
	mergeImportedRules,
	parseRulesExportFile,
	type RuleImportCandidate,
	selectRulesForExport,
} from "../../../utils/smartSelectionExport";
import { RuleImportDialog } from "../../RuleImportDialog/RuleImportDialog";
import { DEFAULT_SMART_SELECTION_RULES, resolveSmartSelectionRules } from "../../Terminal/smartSelectionDefaults";
import type {
	SmartSelectionAction,
	SmartSelectionActionKind,
	SmartSelectionPrecision,
	SmartSelectionRule,
	SmartSelectionRunIn,
	SmartSelectionSendReturn,
	WordSelectionMode,
} from "../../Terminal/smartSelectionTypes";
import { canonicalAction } from "../../Terminal/smartSelectionTypes";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { SettingInput, SettingSelect } from "../SettingFields";
import s from "../Settings.module.css";

/** Built-in default rules indexed by id, for export scope classification and the import
 *  dialog's built-in-vs-custom conflict note. */
const DEFAULT_RULES_BY_ID = new Map(DEFAULT_SMART_SELECTION_RULES.map((r) => [r.id, r]));

const PRECISION_OPTIONS: { value: SmartSelectionPrecision; label: string }[] = [
	{ value: "very_low", label: "Very Low" },
	{ value: "low", label: "Low" },
	{ value: "normal", label: "Normal" },
	{ value: "high", label: "High" },
	{ value: "very_high", label: "Very High" },
];

const ACTION_KIND_OPTIONS: { value: SmartSelectionActionKind; label: string }[] = [
	{ value: "copy", label: "Copy" },
	{ value: "open_url", label: "Open URL" },
	{ value: "open_file", label: "Open File" },
	{ value: "send_text", label: "Send Text" },
	// `run_background` is "Run Command": in the background or, per its "Run" option, in a new
	// terminal. The id predates the second mode. There is no separate "Send Command": it was folded
	// into Send Text's "Press Enter: When safe", and a rule saved with the older `run_command` kind
	// is shown as exactly that (see `canonicalAction`).
	{ value: "run_background", label: "Run Command" },
	{ value: "ask_ai", label: "Ask AI" },
];

/** The older, separate form of "Run Command in a new terminal". An existing rule can still carry
 *  it, so it is listed while an action has it (otherwise the select would show a blank), but it is
 *  never offered for a new one: "Run Command" with "Run: In a new terminal" replaces it, and quotes
 *  the matched text, which this older kind does not. */
const LEGACY_NEW_TERMINAL_OPTION = {
	value: "run_command_new_terminal" as SmartSelectionActionKind,
	label: "Run Command in New Terminal (older form)",
};

function kindOptionsFor(kind: SmartSelectionActionKind) {
	return kind === "run_command_new_terminal"
		? [...ACTION_KIND_OPTIONS, LEGACY_NEW_TERMINAL_OPTION]
		: ACTION_KIND_OPTIONS;
}

const SEND_RETURN_OPTIONS: { value: SmartSelectionSendReturn; label: string }[] = [
	{ value: "never", label: "Never" },
	{ value: "when_safe", label: "When safe" },
];

const RUN_IN_OPTIONS: { value: SmartSelectionRunIn; label: string }[] = [
	{ value: "background", label: "In the background" },
	{ value: "new_terminal", label: "In a new terminal" },
];

function newRule(): SmartSelectionRule {
	return {
		id: randomId("sel-"),
		name: "New rule",
		regex: "",
		precision: "normal",
		enabled: true,
		actions: [],
	};
}

function newAction(): SmartSelectionAction {
	return { kind: "copy", title: "Copy", parameter: "\\0", isDefault: false };
}

/**
 * Single-line row for one rule that expands into the full editor on click —
 * mirrors `SmartPromptsTab`'s `PromptRow`. Editing any field on this rule
 * replaces its object in the store on every keystroke (Solid's store setter
 * only preserves reference identity for array elements that are unchanged —
 * the one actually being edited always gets a fresh proxy, confirmed
 * empirically against `solid-js/store`). The parent renders the rule list
 * with `<Index>`, not `<For>` (which is reference-keyed) — `<Index>` keeps
 * one DOM/component instance per array *position* and only updates the
 * per-slot signal when that position's value changes, so a keystroke's fresh
 * object no longer disposes and recreates this row (and the focused `<input>`
 * inside it) the way it used to. The action grid below does the identical
 * thing one level down, via its own `<Index>` over `props.rule.actions`.
 *
 * `expanded` is still owned by the parent (keyed by the rule's stable `id`,
 * not by array position) rather than as local state here — a rule can move
 * position when an earlier rule is removed, and position-keyed state would
 * follow the slot instead of the rule.
 */
const RuleRow: Component<{
	rule: SmartSelectionRule;
	expanded: boolean;
	onToggleExpand: () => void;
	onUpdate: (patch: Partial<SmartSelectionRule>) => void;
	onRemove: () => void;
	onUpdateAction: (index: number, patch: Partial<SmartSelectionAction>) => void;
	onAddAction: () => void;
	onRemoveAction: (index: number) => void;
}> = (props) => {
	// Memoized so the header badge and the body warning share one compile-and-catch
	// per render instead of each calling `isValidRegex` independently.
	const isRegexInvalid = createMemo(() => Boolean(props.rule.regex) && !isValidRegex(props.rule.regex));

	return (
		<div class={s.ruleCard} data-testid="smart-rule">
			<div
				class={s.ruleHeader}
				role="button"
				tabIndex={0}
				data-testid="smart-rule-header"
				onClick={() => props.onToggleExpand()}
				onKeyDown={onClickKeyDown(() => props.onToggleExpand())}
			>
				<label class={s.ruleHeaderEnabled} onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
					<input
						data-setting-id="app.smart_selection_rules"
						type="checkbox"
						checked={props.rule.enabled}
						onChange={(e) => props.onUpdate({ enabled: e.currentTarget.checked })}
					/>
					Enabled
				</label>
				<span class={s.ruleNameLabel} data-testid="smart-rule-name" style={{ opacity: props.rule.enabled ? 1 : 0.5 }}>
					{props.rule.name || "Unnamed rule"}
				</span>
				<Show when={props.rule.regex}>
					<code class={s.ruleRegexPreview}>{props.rule.regex}</code>
				</Show>
				<Show when={isRegexInvalid()}>
					<span class={s.ruleWarningBadge}>Invalid pattern</span>
				</Show>
			</div>

			<Show when={props.expanded}>
				<div class={s.ruleBody}>
					<div class={s.ruleField}>
						<label class={s.ruleFieldLabel}>Name</label>
						<Input
							data-setting-id="app.smart_selection_rules"
							class={s.ruleName}
							value={props.rule.name}
							placeholder="Name"
							onInput={(e) => props.onUpdate({ name: e.currentTarget.value })}
						/>
					</div>

					<div class={s.ruleField}>
						<label class={s.ruleFieldLabel}>Pattern</label>
						<Input
							data-setting-id="app.smart_selection_rules"
							class={s.ruleRegex}
							value={props.rule.regex}
							placeholder="Regular expression"
							onInput={(e) => props.onUpdate({ regex: e.currentTarget.value })}
						/>
						<Show when={isRegexInvalid()}>
							<p class={s.warning}>Not a valid regular expression — this rule will be skipped.</p>
						</Show>
					</div>

					<div class={s.ruleField}>
						<label class={s.ruleFieldLabel}>Precision</label>
						<Select
							data-setting-id="app.smart_selection_rules"
							class={s.rulePrecision}
							value={props.rule.precision}
							onChange={(e) => props.onUpdate({ precision: e.currentTarget.value as SmartSelectionPrecision })}
						>
							<For each={PRECISION_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
						</Select>
					</div>

					<Show when={props.rule.actions.length > 0}>
						<div class={s.actionGrid}>
							<span class={s.actionGridHeader}>Action</span>
							<span class={s.actionGridHeader}>Menu label</span>
							<span class={s.actionGridHeader}>Parameter</span>
							<span class={s.actionGridHeader}>Default</span>
							<span class={s.actionGridHeader} />
							<Index each={props.rule.actions}>
								{(action, index) => (
									<>
										<Select
											data-setting-id="app.smart_selection_rules"
											value={action().kind}
											data-testid="smart-action-kind"
											onChange={(e) =>
												props.onUpdateAction(index, { kind: e.currentTarget.value as SmartSelectionActionKind })
											}
										>
											<For each={kindOptionsFor(action().kind)}>
												{(opt) => <option value={opt.value}>{opt.label}</option>}
											</For>
										</Select>
										<Input
											data-setting-id="app.smart_selection_rules"
											value={action().title}
											placeholder="Menu label"
											onInput={(e) => props.onUpdateAction(index, { title: e.currentTarget.value })}
										/>
										<Input
											data-setting-id="app.smart_selection_rules"
											class={s.actionParameter}
											value={action().parameter}
											placeholder="\0"
											onInput={(e) => props.onUpdateAction(index, { parameter: e.currentTarget.value })}
										/>
										<label class={s.actionDefault}>
											<input
												data-setting-id="app.smart_selection_rules"
												type="radio"
												name={`default-action-${props.rule.id}`}
												checked={action().isDefault}
												onChange={() => props.onUpdateAction(index, { isDefault: true })}
											/>
											Default
										</label>
										<Button
											data-setting-id="app.smart_selection_rules"
											data-testid="smart-action-remove"
											onClick={() => props.onRemoveAction(index)}
										>
											Remove
										</Button>
										{/* Spans, not labels: the settings search index only reads labels and headings. */}
										<Show when={action().kind === "send_text"}>
											<div class={s.actionOptions}>
												<span>Press Enter</span>
												<Select
													data-setting-id="app.smart_selection_rules"
													data-testid="smart-action-send-return"
													aria-label="Press Enter"
													value={action().sendReturn ?? "never"}
													onChange={(e) =>
														props.onUpdateAction(index, {
															sendReturn: e.currentTarget.value === "when_safe" ? "when_safe" : undefined,
														})
													}
												>
													<For each={SEND_RETURN_OPTIONS}>
														{(opt) => <option value={opt.value}>{opt.label}</option>}
													</For>
												</Select>
												<span class={s.hintInline}>
													“When safe”: always with an agent running; in a plain shell only if the text is plain
													(letters, digits, spaces and _ . / : @ % + = , ~ -).
												</span>
											</div>
										</Show>
										<Show when={action().kind === "run_background"}>
											<div class={s.actionOptions}>
												<span>Run</span>
												<Select
													data-setting-id="app.smart_selection_rules"
													data-testid="smart-action-run-in"
													aria-label="Run"
													value={action().runIn ?? "background"}
													onChange={(e) =>
														props.onUpdateAction(index, {
															runIn: e.currentTarget.value === "new_terminal" ? "new_terminal" : undefined,
														})
													}
												>
													<For each={RUN_IN_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
												</Select>
											</div>
										</Show>
									</>
								)}
							</Index>
						</div>
					</Show>

					{/* A <p>, not a <label>: the settings search index only reads labels and headings. */}
					<Show when={props.rule.actions.some((a) => a.kind === "run_background")}>
						<p class={s.hint} data-testid="smart-run-command-hint">
							Run Command runs from the terminal's folder, as a <code>/bin/sh</code> command. In the background its
							output is discarded — choose “In a new terminal” to watch it (that needs bash, zsh, ksh, dash or sh as
							your shell, and a command line under 1000 bytes). With zsh, bash, fish or ksh as your shell it gets your
							login environment, including your <code>PATH</code>; with another shell it gets the app's. It runs on this
							computer, also for an SSH session. Matched text is handed to the command as data, however you quote{" "}
							<code>\0</code>: it can never become part of the command itself. Don't put it inside <code>$(( ))</code>,{" "}
							<code>[[ -eq ]]</code>, <code>eval</code> or <code>sh -c</code>, which read it as code. A match that
							starts with “-” may be read by the tool as an option; put <code>--</code> before <code>\0</code> if that
							matters. Send Text types into the terminal instead.
						</p>
					</Show>

					{/* A <p>, not a <label>: the settings search index only reads labels and headings. */}
					<Show when={props.rule.actions.some((a) => a.kind === "run_command_new_terminal")}>
						<p class={s.hint} data-testid="smart-legacy-new-terminal-hint">
							The older “Run Command in New Terminal” puts matched text straight into the command line and runs it as
							soon as the terminal opens. Switch the action to Run Command with “In a new terminal”, which hands the
							text to the command as data.
						</p>
					</Show>

					<div style={{ display: "flex", gap: "8px" }}>
						<Button data-setting-id="app.smart_selection_rules" onClick={props.onAddAction}>
							Add action
						</Button>
						<Button
							data-setting-id="app.smart_selection_rules"
							data-testid="smart-rule-remove"
							onClick={props.onRemove}
						>
							Remove rule
						</Button>
					</div>
				</div>
			</Show>
		</div>
	);
};

export const SelectionTab: Component = () => {
	// Which rules are expanded, keyed by the rule's stable `id` rather than by
	// object reference — see `RuleRow`'s docblock for why: editing a rule
	// replaces its object in the store, so identity-keyed state (e.g. a
	// `createSignal` owned by the row itself) would reset on every keystroke.
	const [expandedIds, setExpandedIds] = createSignal<ReadonlySet<string>>(new Set());
	const toggleExpanded = (id: string) =>
		setExpandedIds((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});

	// The stored list is empty until the user customizes something — the
	// editor always shows the REAL effective set (built-ins when empty) so
	// editing one rule doesn't silently discard every other built-in.
	// `actions` defaults to `[]` here — the single place that normalizes it —
	// so every helper below (and the render) can assume a real array instead
	// of each guarding separately against a malformed/hand-edited config.json
	// rule that's missing the field. Returns `r` itself (not a copy) whenever
	// `actions` is already present, so unmodified rules keep a stable object
	// reference across edits, which spares every OTHER row's `RuleRow`
	// instance (and its DOM) from an unnecessary remount. Memoized (not a plain
	// function) so the several handlers/computations that each call `rules()`
	// per interaction — updateRule/removeRule/addRule/updateAction (which calls
	// it twice: once via `.find`, again inside `updateRule`), `scopeCounts`, and
	// export/import — share one resolve+map instead of redoing it per call;
	// Solid recomputes only when `smartSelectionRules` itself changes.
	const rules = createMemo((): SmartSelectionRule[] =>
		resolveSmartSelectionRules(settingsStore.state.smartSelectionRules).map((r) => {
			if (!r.actions) return { ...r, actions: [] };
			// A rule saved with the older `run_command` is shown — and, on the first edit, saved — as
			// Send Text + "Press Enter: When safe" (same behaviour). Only such a rule gets a new
			// object, so every other row keeps its reference and does not remount.
			return r.actions.some((a) => a.kind === "run_command") ? { ...r, actions: r.actions.map(canonicalAction) } : r;
		}),
	);

	const updateRule = (id: string, patch: Partial<SmartSelectionRule>) =>
		settingsStore.setSmartSelectionRules(rules().map((r) => (r.id === id ? { ...r, ...patch } : r)));
	const removeRule = (id: string) => settingsStore.setSmartSelectionRules(rules().filter((r) => r.id !== id));
	// A rule you just added is one you are about to fill in, so open it rather than leaving it
	// as a collapsed row to find and click.
	const addRule = () => {
		const rule = newRule();
		settingsStore.setSmartSelectionRules([...rules(), rule]);
		setExpandedIds((prev) => new Set(prev).add(rule.id));
	};
	const restoreDefaultRules = () => settingsStore.setSmartSelectionRules([]);

	const updateAction = (ruleId: string, index: number, patch: Partial<SmartSelectionAction>) => {
		const rule = rules().find((r) => r.id === ruleId);
		if (!rule) return;
		const actions = rule.actions.map((a, i) => {
			if (i !== index) {
				// Only one action may be marked default — setting this one clears any other.
				return patch.isDefault ? { ...a, isDefault: false } : a;
			}
			// Changing the kind drops the options of the old one: they mean nothing for the new
			// kind, and a hidden leftover would still count as an edit and travel in exports.
			const clearOptions =
				patch.kind !== undefined && patch.kind !== a.kind ? { sendReturn: undefined, runIn: undefined } : {};
			return { ...a, ...clearOptions, ...patch };
		});
		updateRule(ruleId, { actions });
	};
	const addAction = (ruleId: string) => {
		const rule = rules().find((r) => r.id === ruleId);
		if (!rule) return;
		updateRule(ruleId, { actions: [...rule.actions, newAction()] });
	};
	const removeAction = (ruleId: string, index: number) => {
		const rule = rules().find((r) => r.id === ruleId);
		if (!rule) return;
		updateRule(ruleId, { actions: rule.actions.filter((_, i) => i !== index) });
	};

	const [exportScope, setExportScope] = createSignal<ExportScope>("all");
	const [importState, setImportState] = createSignal<{ candidates: RuleImportCandidate[]; warnings: string[] } | null>(
		null,
	);

	/** Counts shown in the export scope dropdown, so the choice is obvious before a file is written. */
	const scopeCounts = createMemo(() => {
		const all = rules();
		return {
			all: all.length,
			modified: selectRulesForExport(all, "modified", DEFAULT_RULES_BY_ID).length,
			custom: selectRulesForExport(all, "custom", DEFAULT_RULES_BY_ID).length,
		};
	});

	const handleExport = async () => {
		const scope = exportScope();
		const selected = selectRulesForExport(rules(), scope, DEFAULT_RULES_BY_ID);
		const file = buildRulesExportFile(selected, scope);
		await exportJsonWithToast(
			`smart-selection-rules-${scope}.json`,
			file,
			"Export Smart Selection Rules",
			"Exported Smart Selection rules",
		);
	};

	const handleImportFile = async () => {
		const text = await pickJsonImportFile();
		if (text === null) return;
		const { rules: parsedRules, warnings, error } = parseRulesExportFile(text);
		if (error) {
			toastsStore.add("Import failed", error, "error");
			return;
		}
		if (parsedRules.length === 0) {
			toastsStore.add("Nothing to import", warnings[0] ?? "The file contained no rules", "warn");
			return;
		}
		const existingIds = new Set(rules().map((r) => r.id));
		const candidates = classifyRuleImport(parsedRules, existingIds);
		setImportState({ candidates, warnings });
	};

	const handleImportConfirm = (selectedIds: string[]) => {
		const state = importState();
		if (!state) return;
		const selected = new Set(selectedIds);
		const toImport = state.candidates.filter((c) => selected.has(c.rule.id)).map((c) => c.rule);
		const { merged, disabled } = mergeImportedRules(rules(), toImport);
		settingsStore.setSmartSelectionRules(merged);
		setImportState(null);
		toastsStore.add(
			`Imported ${toImport.length} rule${toImport.length === 1 ? "" : "s"}`,
			disabled.length > 0 ? `Imported disabled (review before enabling): ${disabled.join(", ")}` : "",
			disabled.length > 0 ? "warn" : "info",
		);
	};

	return (
		<div class={s.section}>
			<h3>Behavior</h3>

			<SettingSelect
				settingId="app.double_click_action"
				label="Double-click performs"
				value={settingsStore.state.doubleClickAction}
				onChange={(v) => settingsStore.setDoubleClickAction(v as "word" | "smart")}
				options={[
					{ value: "word", label: "Word selection" },
					{ value: "smart", label: "Smart selection" },
				]}
				hint="Word selection expands to the character-class boundary below. Smart selection tries the rule list first, falling back to word selection when nothing matches. Quad-click (4 rapid clicks) and the right-click smart-selection menu always try the rule list, regardless of this setting."
			/>

			<h3>Word Boundaries</h3>

			<SettingSelect
				settingId="app.word_selection_mode"
				label="Word boundaries"
				value={settingsStore.state.wordSelectionMode}
				onChange={(v) => settingsStore.setWordSelectionMode(v as WordSelectionMode)}
				options={[
					{ value: "characters", label: "Character list" },
					{ value: "regex", label: "Regular expression" },
				]}
				hint="Character list: a literal set of characters that BREAK a word (today's punctuation set, by default). Regular expression: `|`-joined alternates — the longest match at each position joins onto the adjacent word, e.g. adding https:// lets a double-click on a URL's host include the scheme."
			/>

			<Show when={settingsStore.state.wordSelectionMode === "characters"}>
				<SettingInput
					settingId="app.word_separators"
					label="Word separators"
					value={settingsStore.state.wordSeparators}
					onInput={(v) => settingsStore.setWordSeparators(v)}
					hint="Characters that break a word for double-click selection. Whitespace and control characters are always separators regardless of this list."
				/>
				<Button
					data-setting-id="app.word_separators"
					onClick={() => settingsStore.setWordSeparators(DEFAULT_WORD_SEPARATORS)}
				>
					Restore default separators
				</Button>
			</Show>

			<Show when={settingsStore.state.wordSelectionMode === "regex"}>
				<SettingInput
					settingId="app.word_selection_regex"
					label="Word pattern"
					value={settingsStore.state.wordSelectionRegex}
					onInput={(v) => settingsStore.setWordSelectionRegex(v)}
					placeholder="https://|-|_"
					hint="`|`-joined alternates. Plain letters/digits/underscore are always word characters; add alternates here to join punctuation-containing spans onto them."
				/>
				<Show when={settingsStore.state.wordSelectionRegex && !isValidRegex(settingsStore.state.wordSelectionRegex)}>
					<p class={s.warning}>One or more alternates are not valid regular expressions and will be skipped.</p>
				</Show>
			</Show>

			<h3>Smart Selection Rules</h3>
			<p class={s.hint}>
				Each rule's regex is scored by precision × match length — the highest-scoring match spanning the click wins. A
				rule's actions appear in the right-click menu when its match is under the cursor; the action marked "Default"
				runs on Option/Alt+double-click.
			</p>

			<div class={s.transferRow}>
				<Select
					data-non-setting="Chooses which rules the Export action writes"
					data-testid="rule-export-scope-select"
					value={exportScope()}
					onChange={(e) => setExportScope(e.currentTarget.value as ExportScope)}
				>
					<option value="all">All rules ({scopeCounts().all})</option>
					<option value="modified">Modified only ({scopeCounts().modified})</option>
					<option value="custom">Custom only ({scopeCounts().custom})</option>
				</Select>
				<Button
					data-non-setting="Exports Smart Selection rules to a file"
					data-testid="rule-export-btn"
					onClick={handleExport}
				>
					Export…
				</Button>
				<Button
					data-non-setting="Imports Smart Selection rules from a file"
					data-testid="rule-import-btn"
					onClick={handleImportFile}
				>
					Import…
				</Button>
			</div>

			<Index each={rules()}>
				{(rule) => (
					<RuleRow
						rule={rule()}
						expanded={expandedIds().has(rule().id)}
						onToggleExpand={() => toggleExpanded(rule().id)}
						onUpdate={(patch) => updateRule(rule().id, patch)}
						onRemove={() => removeRule(rule().id)}
						onUpdateAction={(index, patch) => updateAction(rule().id, index, patch)}
						onAddAction={() => addAction(rule().id)}
						onRemoveAction={(index) => removeAction(rule().id, index)}
					/>
				)}
			</Index>
			<div style={{ display: "flex", gap: "8px" }}>
				<Button data-setting-id="app.smart_selection_rules" onClick={addRule}>
					Add rule
				</Button>
				<Button data-setting-id="app.smart_selection_rules" onClick={restoreDefaultRules}>
					Restore built-in defaults
				</Button>
			</div>

			<Show when={importState()}>
				{(state) => (
					<RuleImportDialog
						candidates={state().candidates}
						warnings={state().warnings}
						willMaterializeDefaults={settingsStore.state.smartSelectionRules.length === 0}
						isBuiltIn={(id) => DEFAULT_RULES_BY_ID.has(id)}
						onImport={handleImportConfirm}
						onCancel={() => setImportState(null)}
					/>
				)}
			</Show>
		</div>
	);
};
