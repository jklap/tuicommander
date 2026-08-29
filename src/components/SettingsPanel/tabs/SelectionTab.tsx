import { type Component, For, Show } from "solid-js";
import { DEFAULT_WORD_SEPARATORS, settingsStore } from "../../../stores/settings";
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
		id: crypto.randomUUID(),
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

export const SelectionTab: Component = () => {
	// The stored list is empty until the user customizes something — the
	// editor always shows the REAL effective set (built-ins when empty) so
	// editing one rule doesn't silently discard every other built-in.
	// `actions` defaults to `[]` here — the single place that normalizes it —
	// so every helper below (and the render) can assume a real array instead
	// of each guarding separately against a malformed/hand-edited config.json
	// rule that's missing the field.
	const rules = (): SmartSelectionRule[] =>
		resolveSmartSelectionRules(settingsStore.state.smartSelectionRules).map((r) => ({
			...r,
			actions: r.actions ?? [],
		}));

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

			<SettingToggle
				checked={settingsStore.state.smartSelectionEnabled}
				onChange={(v) => settingsStore.setSmartSelectionEnabled(v)}
				label="Enable smart selection"
				hint="Try the rule list below before falling back to plain word-boundary selection. Word-boundary customization still applies when this is off."
			/>

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

			<h3>Smart Selection Rules</h3>
			<p class={s.hint}>
				Each rule's regex is scored by precision × match length — the highest-scoring match spanning the click wins.
				A rule's actions appear in the right-click menu when its match is under the
				cursor; the action marked "Default" runs on Option/Alt+double-click.
			</p>

			<For each={rules()}>
				{(rule) => (
					<div class={s.ruleCard} data-testid="smart-rule">
						<div class={s.ruleHeader}>
							<label class={s.ruleHeaderEnabled}>
								<input
									type="checkbox"
									checked={rule.enabled}
									onChange={(e) => updateRule(rule.id, { enabled: e.currentTarget.checked })}
								/>
								Enabled
							</label>
							<input
								type="text"
								class={s.ruleName}
								value={rule.name}
								placeholder="Name"
								data-testid="smart-rule-name"
								onInput={(e) => updateRule(rule.id, { name: e.currentTarget.value })}
							/>
							<button class={s.testBtn} data-testid="smart-rule-remove" onClick={() => removeRule(rule.id)}>
								Remove
							</button>
						</div>

						<div class={s.ruleField}>
							<label class={s.ruleFieldLabel}>Pattern</label>
							<input
								type="text"
								class={s.ruleRegex}
								value={rule.regex}
								placeholder="Regular expression"
								onInput={(e) => updateRule(rule.id, { regex: e.currentTarget.value })}
							/>
							<Show when={rule.regex && !isValidRegex(rule.regex)}>
								<p class={s.warning}>Not a valid regular expression — this rule will be skipped.</p>
							</Show>
						</div>

						<div class={s.ruleField}>
							<label class={s.ruleFieldLabel}>Precision</label>
							<select
								class={s.rulePrecision}
								value={rule.precision}
								onChange={(e) => updateRule(rule.id, { precision: e.currentTarget.value as SmartSelectionPrecision })}
							>
								<For each={PRECISION_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
							</select>
						</div>

						<Show when={rule.actions.length > 0}>
							<div class={s.actionGrid}>
								<span class={s.actionGridHeader}>Action</span>
								<span class={s.actionGridHeader}>Menu label</span>
								<span class={s.actionGridHeader}>Parameter</span>
								<span class={s.actionGridHeader}>Default</span>
								<span class={s.actionGridHeader} />
								<For each={rule.actions}>
									{(action, index) => (
										<>
											<select
												value={action.kind}
												data-testid="smart-action-kind"
												onChange={(e) =>
													updateAction(rule.id, index(), { kind: e.currentTarget.value as SmartSelectionActionKind })
												}
											>
												<For each={ACTION_KIND_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
											</select>
											<input
												type="text"
												value={action.title}
												placeholder="Menu label"
												onInput={(e) => updateAction(rule.id, index(), { title: e.currentTarget.value })}
											/>
											<input
												type="text"
												class={s.actionParameter}
												value={action.parameter}
												placeholder="\0"
												onInput={(e) => updateAction(rule.id, index(), { parameter: e.currentTarget.value })}
											/>
											<label class={s.actionDefault}>
												<input
													type="radio"
													name={`default-action-${rule.id}`}
													checked={action.isDefault}
													onChange={() => updateAction(rule.id, index(), { isDefault: true })}
												/>
												Default
											</label>
											<button
												class={s.testBtn}
												data-testid="smart-action-remove"
												onClick={() => removeAction(rule.id, index())}
											>
												Remove
											</button>
										</>
									)}
								</For>
							</div>
						</Show>
						<button class={s.testBtn} onClick={() => addAction(rule.id)}>
							Add action
						</button>
					</div>
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
