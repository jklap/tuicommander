import { type Component, createMemo, createSignal, For, Show } from "solid-js";
import { DEFAULT_WORD_SEPARATORS, settingsStore } from "../../../stores/settings";
import { onClickKeyDown } from "../../../utils/a11y";
import { randomId } from "../../../utils/randomId";
import { resolveSmartSelectionRules } from "../../Terminal/smartSelectionDefaults";
import type {
	SmartSelectionAction,
	SmartSelectionActionKind,
	SmartSelectionPrecision,
	SmartSelectionRule,
	WordSelectionMode,
} from "../../Terminal/smartSelectionTypes";
import { SettingInput, SettingSelect, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

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
	{ value: "run_command", label: "Run Command" },
	{ value: "run_command_new_terminal", label: "Run Command in New Terminal" },
	{ value: "ask_ai", label: "Ask AI" },
];

function isValidRegex(pattern: string): boolean {
	try {
		new RegExp(pattern);
		return true;
	} catch {
		return false;
	}
}

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
 * mirrors `SmartPromptsTab`'s `PromptRow`. Unlike `PromptRow`, `expanded` is
 * NOT local `createSignal` state owned by this component instance: editing
 * any field on this same rule replaces its object in the store (Solid's
 * store setter only preserves reference identity for array elements that
 * are unchanged — the one actually being edited always gets a fresh proxy,
 * confirmed empirically against `solid-js/store`), which would make `<For>`
 * remount this component and reset a local signal on every keystroke,
 * collapsing the editor out from under the user typing into it. Owning
 * `expanded` in the parent instead, keyed by the rule's stable `id`, survives
 * that remount entirely.
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
						<input
							type="text"
							value={props.rule.name}
							placeholder="Name"
							onInput={(e) => props.onUpdate({ name: e.currentTarget.value })}
						/>
					</div>

					<div class={s.ruleField}>
						<label class={s.ruleFieldLabel}>Pattern</label>
						<input
							type="text"
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
						<select
							class={s.rulePrecision}
							value={props.rule.precision}
							onChange={(e) => props.onUpdate({ precision: e.currentTarget.value as SmartSelectionPrecision })}
						>
							<For each={PRECISION_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
						</select>
					</div>

					<Show when={props.rule.actions.length > 0}>
						<div class={s.actionGrid}>
							<span class={s.actionGridHeader}>Action</span>
							<span class={s.actionGridHeader}>Menu label</span>
							<span class={s.actionGridHeader}>Parameter</span>
							<span class={s.actionGridHeader}>Default</span>
							<span class={s.actionGridHeader} />
							<For each={props.rule.actions}>
								{(action, index) => (
									<>
										<select
											value={action.kind}
											data-testid="smart-action-kind"
											onChange={(e) =>
												props.onUpdateAction(index(), { kind: e.currentTarget.value as SmartSelectionActionKind })
											}
										>
											<For each={ACTION_KIND_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
										</select>
										<input
											type="text"
											value={action.title}
											placeholder="Menu label"
											onInput={(e) => props.onUpdateAction(index(), { title: e.currentTarget.value })}
										/>
										<input
											type="text"
											class={s.actionParameter}
											value={action.parameter}
											placeholder="\0"
											onInput={(e) => props.onUpdateAction(index(), { parameter: e.currentTarget.value })}
										/>
										<label class={s.actionDefault}>
											<input
												type="radio"
												name={`default-action-${props.rule.id}`}
												checked={action.isDefault}
												onChange={() => props.onUpdateAction(index(), { isDefault: true })}
											/>
											Default
										</label>
										<button
											class={s.testBtn}
											data-testid="smart-action-remove"
											onClick={() => props.onRemoveAction(index())}
										>
											Remove
										</button>
									</>
								)}
							</For>
						</div>
					</Show>

					<div style={{ display: "flex", gap: "8px" }}>
						<button class={s.testBtn} onClick={props.onAddAction}>
							Add action
						</button>
						<button class={s.testBtn} data-testid="smart-rule-remove" onClick={props.onRemove}>
							Remove rule
						</button>
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
	// instance (and its DOM) from an unnecessary remount.
	const rules = (): SmartSelectionRule[] =>
		resolveSmartSelectionRules(settingsStore.state.smartSelectionRules).map((r) =>
			r.actions ? r : { ...r, actions: [] },
		);

	const updateRule = (id: string, patch: Partial<SmartSelectionRule>) =>
		settingsStore.setSmartSelectionRules(rules().map((r) => (r.id === id ? { ...r, ...patch } : r)));
	const removeRule = (id: string) => settingsStore.setSmartSelectionRules(rules().filter((r) => r.id !== id));
	const addRule = () => settingsStore.setSmartSelectionRules([...rules(), newRule()]);
	const restoreDefaultRules = () => settingsStore.setSmartSelectionRules([]);

	const updateAction = (ruleId: string, index: number, patch: Partial<SmartSelectionAction>) => {
		const rule = rules().find((r) => r.id === ruleId);
		if (!rule) return;
		const actions = rule.actions.map((a, i) => {
			if (i !== index) {
				// Only one action may be marked default — setting this one clears any other.
				return patch.isDefault ? { ...a, isDefault: false } : a;
			}
			return { ...a, ...patch };
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

	return (
		<div class={s.section}>
			<h3>Behavior</h3>

			<SettingSelect
				label="Double-click performs"
				value={settingsStore.state.doubleClickAction}
				onChange={(v) => settingsStore.setDoubleClickAction(v as "word" | "smart")}
				options={[
					{ value: "word", label: "Word selection" },
					{ value: "smart", label: "Smart selection" },
				]}
				hint="Word selection expands to the character-class boundary below. Smart selection tries the rule list first, falling back to word selection when nothing matches. Quad-click (4 rapid clicks) always tries smart selection, regardless of this setting."
			/>

			<h3>Word Boundaries</h3>

			<SettingSelect
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
					label="Word separators"
					value={settingsStore.state.wordSeparators}
					onInput={(v) => settingsStore.setWordSeparators(v)}
					hint="Characters that break a word for double-click selection. Whitespace and control characters are always separators regardless of this list."
				/>
				<button class={s.testBtn} onClick={() => settingsStore.setWordSeparators(DEFAULT_WORD_SEPARATORS)}>
					Restore default separators
				</button>
			</Show>

			<Show when={settingsStore.state.wordSelectionMode === "regex"}>
				<SettingInput
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

			<SettingToggle
				checked={settingsStore.state.smartSelectionEnabled}
				onChange={(v) => settingsStore.setSmartSelectionEnabled(v)}
				label="Enable smart selection"
				hint="Try the rule list below before falling back to plain word-boundary selection. Word-boundary customization still applies when this is off."
			/>

			<h3>Smart Selection Rules</h3>
			<p class={s.hint}>
				Each rule's regex is scored by precision × match length — the highest-scoring match spanning the click wins. A
				rule's actions appear in the right-click menu when its match is under the cursor; the action marked "Default"
				runs on Option/Alt+double-click.
			</p>

			<For each={rules()}>
				{(rule) => (
					<RuleRow
						rule={rule}
						expanded={expandedIds().has(rule.id)}
						onToggleExpand={() => toggleExpanded(rule.id)}
						onUpdate={(patch) => updateRule(rule.id, patch)}
						onRemove={() => removeRule(rule.id)}
						onUpdateAction={(index, patch) => updateAction(rule.id, index, patch)}
						onAddAction={() => addAction(rule.id)}
						onRemoveAction={(index) => removeAction(rule.id, index)}
					/>
				)}
			</For>
			<div style={{ display: "flex", gap: "8px" }}>
				<button class={s.testBtn} onClick={addRule}>
					Add rule
				</button>
				<button class={s.testBtn} onClick={restoreDefaultRules}>
					Restore built-in defaults
				</button>
			</div>
		</div>
	);
};
