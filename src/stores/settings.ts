import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import appChoiceOptions from "../../src-tauri/src/settings/app_choice_options.json";
import type { LinkActivation } from "../components/Terminal/canvasTerminalLinks";
import {
	type DoubleClickAction,
	type RustSmartSelectionRule,
	ruleFromWire,
	ruleToWire,
	type SmartSelectionRule,
	type WordSelectionMode,
} from "../components/Terminal/smartSelectionTypes";
import { setLocale } from "../i18n";
import type { IndicatorOverride } from "../indicators/registry";
import {
	isSafeIndicatorAnimationId,
	isSafeIndicatorColor,
	isSafeIndicatorIconId,
	sanitizeIndicatorOverrides,
} from "../indicators/validate";
import { invoke } from "../invoke";
import { settingNumberDefault } from "../services/settingsCatalogue";
import { settingsRefresh } from "../services/settingsRefresh";
import type { IssueFilterMode } from "../types";
import { isAcpConfigured } from "../utils/acpAvailability";
import { isValidEnvVarKey } from "../utils/envVars";
import { runSerializedConfigWrite, updateAppConfig } from "../utils/updateAppConfig";
import { appLogger } from "./appLogger";
import { toastsStore } from "./toasts";

// Legacy storage keys for one-time migration
const LEGACY_KEYS = {
	IDE: "tui-commander-default-ide",
	SESSION: "tui-commander-session",
} as const;

/** Rust AppConfig shape (subset needed for font/ide read/write) */
/** A user-defined launcher for the "Open in" menu (GH #71). */
export interface CustomLauncher {
	id: string;
	name: string;
	/** Executable: bare name (resolved on PATH) or absolute path. */
	executable: string;
	/** Args; each may contain {path}/{file}/{line}/{column} placeholders. */
	args: string[];
	enabled: boolean;
	/** Optional platform filter: "macos" | "windows" | "linux". undefined = all. */
	platform?: "macos" | "windows" | "linux";
}

/** One user-authored `KEY=value` pair for `customPtyEnv` — mirrors Rust's
 *  `CustomEnvVarEntry` (`src-tauri/src/config.rs`) exactly; same shape as
 *  `src/utils/envVars.ts`'s `EnvVarEntry`, so that module's validation helpers
 *  apply directly with no adaptation. */
export interface CustomEnvVarEntry {
	key: string;
	value: string;
}

/** Drop malformed keys and collapse duplicate keys (first occurrence wins) from
 *  a hand-edited `config.json`'s `custom_pty_env` — same "revalidate on
 *  hydrate, not just on write" precedent as `sanitizeIndicatorOverrides`, since
 *  this is untrusted input reaching real process environment variables.
 *  Case-insensitive dedup: Windows env var names collide case-insensitively at
 *  the OS level, so "PATH" and "Path" as two separate entries would silently
 *  let one clobber the other at spawn time with no indication which. */
function sanitizeCustomPtyEnv(entries: CustomEnvVarEntry[]): CustomEnvVarEntry[] {
	const seen = new Set<string>();
	const out: CustomEnvVarEntry[] = [];
	for (const entry of entries) {
		const normalized = entry.key.toLowerCase();
		if (!isValidEnvVarKey(entry.key) || seen.has(normalized)) continue;
		seen.add(normalized);
		out.push({ key: entry.key, value: entry.value });
	}
	return out;
}

export type TrayIconPreference = "always" | "background_only" | "never";
export type CloseBehavior = "ask" | "background" | "quit";

interface RustAppConfig {
	shell: string | null;
	font_family: string;
	font_size: number;
	font_weight: number;
	theme: string;
	mcp_server_enabled: boolean;
	ide: string;
	default_font_size: number;
	close_behavior: CloseBehavior;
	background?: { hide_dock_icon?: boolean; tray_icon?: TrayIconPreference; webview_unload_minutes?: number };
	confirm_before_quit: boolean;
	confirm_before_closing_tab: boolean;
	restore_window_geometry?: boolean;
	restore_shell_terminals?: boolean;
	restore_scrollback?: boolean;
	restore_scrollback_lines?: number;
	max_tab_name_length: number;
	split_tab_mode: string;
	tab_ordering_mode: string;
	tab_cycling_all_types: boolean;
	tab_tree_enabled: boolean;
	chat_mute_intermediate_text: boolean;
	auto_show_pr_popover: boolean;
	prevent_sleep_when_busy: boolean;
	auto_update_enabled: boolean;
	auto_update_plugins_enabled: boolean;
	defer_update_relaunch: boolean;
	eager_resume_after_update: boolean;
	language: string;
	update_channel: string;
	services: {
		auth: { session_token_duration_secs: number };
	};
	disabled_agents: string[];
	intent_tab_title: boolean;
	progress_tracking: boolean;
	suggest_followups: boolean;
	copy_on_select: boolean;
	osc52_clipboard: boolean;
	osc1337_focus_attention: boolean;
	show_last_prompt: boolean;
	bell_style: string;
	global_hotkey: string | null;
	issue_filter?: string;
	pr_hide_drafts?: boolean;
	pr_hide_conflicting?: boolean;
	pr_hide_ci_failing?: boolean;
	experimental_features_enabled?: boolean;
	scrollback_reflow?: boolean;
	cursor_style?: string;
	terminal_renderer?: string;
	terminal_link_activation?: string;
	double_click_action?: string;
	word_selection_mode?: string;
	word_separators?: string;
	word_selection_regex?: string;
	smart_selection_rules?: RustSmartSelectionRule[];
	/** @deprecated superseded by `block_timestamp_mode` (issue #5) — kept only so a config
	 *  saved by an older build still deserializes; no longer written by `save()`. */
	show_block_timestamps?: boolean;
	block_timestamp_mode?: string;
	show_block_marks?: boolean;
	show_prompt_marks?: boolean;
	block_folding_enabled?: boolean;
	index_strategy?: string;
	index_memory_budget_mb?: number;
	attachment_max_bytes?: number;
	standby_timeout_minutes?: number;
	custom_launchers?: CustomLauncher[];
	additional_readable_dirs?: string[];
	custom_pty_env?: CustomEnvVarEntry[];
	diff_ignore_leading_whitespace?: boolean;
	diff_ignore_trailing_whitespace?: boolean;
	diff_ignore_whitespace_amount?: boolean;
	diff_ignore_case?: boolean;
	session_diff_auto_open?: string;
	session_diff_truncate_lines?: number;
	inline_blame_enabled?: boolean;
	/** The one binary the host may launch for ACP. Empty means ACP is unconfigured. */
	ego_executable?: string;
	/** Optional ego user-config profile for AI Chat ACP launches. */
	ego_profile?: string;
	ai_chat_workspace?: string;
	indicator_overrides?: IndicatorOverride[];
	show_diff_stats?: boolean;
	show_pr_badges?: boolean;
	show_git_state?: boolean;
	tab_type_highlighting?: boolean;
}

// Default values
/** Theme used when the configured key is empty or names no loaded theme.
 *  Lives here, not in themes.ts: themes.ts imports this store. */
export const DEFAULT_THEME = "commander";

const DEFAULTS = {
	ide: "vscode" as const,
	font: "JetBrains Mono" as const,
	fontSize: 13,
	fontWeight: 400,
};

/** IDE options */
export type IdeType =
	| "vscode"
	| "cursor"
	| "zed"
	| "windsurf"
	| "neovim"
	| "xcode"
	| "ghostty"
	| "wezterm"
	| "alacritty"
	| "kitty"
	| "warp"
	| "iterm2"
	| "sourcetree"
	| "github-desktop"
	| "fork"
	| "gitkraken"
	| "smerge"
	| "tower"
	| "intellij"
	| "pycharm"
	| "webstorm"
	| "goland"
	| "clion"
	| "phpstorm"
	| "rubymine"
	| "rider"
	| "datagrip"
	| "rustrover"
	| "android-studio"
	| "fleet"
	| "terminal"
	| "finder"
	| "editor";

/** Font family options (all bundled as woff2) */
export type FontType =
	| "JetBrains Mono"
	| "Fira Code"
	| "Hack"
	| "Cascadia Code"
	| "Iosevka"
	| "Source Code Pro"
	| "Inconsolata"
	| "IBM Plex Mono"
	| "Monaspace Neon"
	| "Commit Mono"
	| "Geist Mono";

/** IDE display names */
export const IDE_NAMES: Record<IdeType, string> = appChoiceOptions.ides;

/** IDE categories */
export const IDE_CATEGORIES: Record<string, IdeType[]> = {
	editors: ["vscode", "cursor", "zed", "windsurf", "neovim", "xcode", "editor"],
	jetbrains: [
		"intellij",
		"pycharm",
		"webstorm",
		"goland",
		"clion",
		"phpstorm",
		"rubymine",
		"rider",
		"datagrip",
		"rustrover",
		"android-studio",
		"fleet",
	],
	terminals: ["ghostty", "wezterm", "alacritty", "kitty", "warp", "iterm2"],
	git: ["sourcetree", "github-desktop", "fork", "gitkraken", "smerge", "tower"],
	utilities: ["terminal", "finder"],
};

/** Font family CSS values — system Nerd Font (if installed) → bundled font → bundled symbols → monospace */
export const FONT_FAMILIES: Record<FontType, string> = appChoiceOptions.fonts;

/** Valid IDE values */
const VALID_IDES: readonly string[] = Object.keys(IDE_NAMES);

/** Valid font values */
const VALID_FONTS = Object.keys(FONT_FAMILIES) as FontType[];

/** Validate and return IDE type or default */
function validateIde(value: string | null): IdeType {
	return value && VALID_IDES.includes(value) ? (value as IdeType) : DEFAULTS.ide;
}

/** Validate and return font type or default */
function validateFont(value: string | null): FontType {
	return value && VALID_FONTS.includes(value as FontType) ? (value as FontType) : DEFAULTS.font;
}

/** Valid issue filter values */
const VALID_ISSUE_FILTERS: readonly IssueFilterMode[] = ["assigned", "created", "mentioned", "all", "disabled"];

/** Validate and return issue filter or default */
function validateIssueFilter(value: string | null): IssueFilterMode {
	return value && (VALID_ISSUE_FILTERS as readonly string[]).includes(value) ? (value as IssueFilterMode) : "assigned";
}

/** Valid terminal renderer values */
const VALID_RENDERERS: readonly TerminalRenderer[] = ["webgl", "canvas", "native"];

function validateTerminalRenderer(value: string | null): TerminalRenderer {
	return value && (VALID_RENDERERS as readonly string[]).includes(value) ? (value as TerminalRenderer) : "webgl";
}

/** Valid link-activation values */
const VALID_LINK_ACTIVATIONS: readonly LinkActivation[] = ["click", "modifier", "never"];

function validateLinkActivation(value: string | null): LinkActivation {
	return value && (VALID_LINK_ACTIVATIONS as readonly string[]).includes(value) ? (value as LinkActivation) : "click";
}

/** Default punctuation separators for "characters" word-selection mode —
 *  matches `WORD_SEPARATOR_RE` in `canvasTerminalSelection.ts` and the Rust
 *  `default_word_separators()` exactly, so the default preserves today's
 *  double-click behavior losslessly. Whitespace/control chars are always
 *  separators regardless of this string. Exported for the Settings UI's
 *  "Restore default" action. */
export const DEFAULT_WORD_SEPARATORS = " \"'`(){}[]<>|;:,.!?@#$%^&*~=+/\\";

/** Valid double-click-action values */
const VALID_DOUBLE_CLICK_ACTIONS: readonly DoubleClickAction[] = ["word", "smart"];

function validateDoubleClickAction(value: string | null): DoubleClickAction {
	return value && (VALID_DOUBLE_CLICK_ACTIONS as readonly string[]).includes(value)
		? (value as DoubleClickAction)
		: "smart";
}

/** Valid word-selection-mode values */
const VALID_WORD_SELECTION_MODES: readonly WordSelectionMode[] = ["characters", "regex"];

function validateWordSelectionMode(value: string | null): WordSelectionMode {
	return value && (VALID_WORD_SELECTION_MODES as readonly string[]).includes(value)
		? (value as WordSelectionMode)
		: "characters";
}

/** Command-block timestamp overlay display mode (issue #5). "modifier" (hold
 *  Ctrl+Cmd to reveal) was the only behavior before this field existed, so it's
 *  the fallback whenever migration can't determine otherwise. */
export type BlockTimestampMode = "off" | "always" | "modifier";
const VALID_BLOCK_TIMESTAMP_MODES: readonly BlockTimestampMode[] = ["off", "always", "modifier"];

/**
 * Resolves `block_timestamp_mode`, migrating a pre-existing config that only
 * ever had the legacy `show_block_timestamps` boolean: `true` becomes
 * "modifier" (today's only behavior, preserved), `false` becomes "off". A
 * config that already carries a valid `block_timestamp_mode` always wins —
 * migration only fires for a config saved before this field existed.
 */
function resolveBlockTimestampMode(
	mode: string | null | undefined,
	legacyBool: boolean | undefined,
): BlockTimestampMode {
	if (mode && (VALID_BLOCK_TIMESTAMP_MODES as readonly string[]).includes(mode)) return mode as BlockTimestampMode;
	return legacyBool === false ? "off" : "modifier";
}

/** Split tab mode */
export type SplitTabMode = "separate" | "unified";

/** Tab ordering mode */
export type TabOrderingMode = "grouped-by-type" | "terminals-first" | "free";

/** Terminal renderer backend */
export type TerminalRenderer = "webgl" | "canvas" | "native";

/** Update channel */
export type UpdateChannel = "stable" | "beta" | "nightly";

/** Settings store state */
interface SettingsStoreState {
	ide: IdeType;
	font: FontType;
	fontWeight: number;
	defaultFontSize: number;
	shell: string | null;
	theme: string;
	closeBehavior: CloseBehavior;
	hideDockIcon: boolean;
	trayIcon: TrayIconPreference;
	confirmBeforeQuit: boolean;
	confirmBeforeClosingTab: boolean;
	restoreWindowGeometry: boolean;
	restoreShellTerminals: boolean;
	restoreScrollback: boolean;
	restoreScrollbackLines: number;
	maxTabNameLength: number;
	splitTabMode: SplitTabMode;
	tabOrderingMode: TabOrderingMode;
	tabCyclingAllTypes: boolean;
	tabTreeEnabled: boolean;
	/** Draw assistant text that a tool call follows in the same turn muted, in Chat. */
	chatMuteIntermediateText: boolean;
	autoShowPrPopover: boolean;
	preventSleepWhenBusy: boolean;
	autoUpdateEnabled: boolean;
	autoUpdatePluginsEnabled: boolean;
	/** Hold the post-update restart until no session is busy or awaiting input */
	deferUpdateRelaunch: boolean;
	/** After an update restart, resume every agent tab with a confirmed session, on every branch */
	eagerResumeAfterUpdate: boolean;
	language: string;
	updateChannel: UpdateChannel;
	disabledAgents: string[];
	intentTabTitle: boolean;
	progressTracking: boolean;
	suggestFollowups: boolean;
	copyOnSelect: boolean;
	osc52Clipboard: boolean;
	osc1337FocusAttention: boolean;
	showLastPrompt: boolean;
	bellStyle: "none" | "visual" | "sound" | "both";
	globalHotkey: string | null;
	issueFilter: IssueFilterMode;
	prHideDrafts: boolean;
	prHideConflicting: boolean;
	prHideCiFailing: boolean;
	experimentalFeaturesEnabled: boolean;
	scrollbackReflow: boolean;
	cursorStyle: "bar" | "block" | "underline";
	terminalRenderer: TerminalRenderer;
	linkActivation: LinkActivation;
	doubleClickAction: DoubleClickAction;
	wordSelectionMode: WordSelectionMode;
	wordSeparators: string;
	wordSelectionRegex: string;
	smartSelectionRules: SmartSelectionRule[];
	blockTimestampMode: BlockTimestampMode;
	/** Scrollbar history ticks, per category (the old `show_scrollbar_marks` master
	 *  toggle was retired; Rust migrates an old `false` into both on load). */
	showBlockMarks: boolean;
	showPromptMarks: boolean;
	blockFoldingEnabled: boolean;
	indexStrategy: "disabled" | "active_only" | "active_and_switch" | "all_sequential";
	standbyTimeoutMinutes: number;
	indexMemoryBudgetMb: number;
	attachmentMaxBytes: number;
	webviewUnloadMinutes: number;
	customLaunchers: CustomLauncher[];
	additionalReadableDirs: string[];
	customPtyEnv: CustomEnvVarEntry[];
	inlineBlameEnabled: boolean;
	/**
	 * Path to the ego binary the AI Chat panel talks ACP to.
	 *
	 * Empty is a meaningful value, not a missing one: Rust refuses every connect
	 * while it is empty, and the panel says so rather than launching nothing.
	 */
	egoExecutable: string;
	egoProfile: string;
	aiChatWorkspace: string;
	diffIgnoreLeadingWhitespace: boolean;
	diffIgnoreTrailingWhitespace: boolean;
	diffIgnoreWhitespaceAmount: boolean;
	diffIgnoreCase: boolean;
	sessionDiffAutoOpen: "off" | "ask" | "auto";
	sessionDiffTruncateLines: number;
	indicatorOverrides: IndicatorOverride[];
	showDiffStats: boolean;
	showPrBadges: boolean;
	showGitState: boolean;
	tabTypeHighlighting: boolean;
}

const SAVE_DEBOUNCE_MS = 500;

/** Create the settings store */
function createSettingsStore() {
	const [state, setState] = createStore<SettingsStoreState>({
		ide: DEFAULTS.ide,
		font: DEFAULTS.font,
		fontWeight: DEFAULTS.fontWeight,
		defaultFontSize: DEFAULTS.fontSize,
		shell: null,
		theme: DEFAULT_THEME,
		closeBehavior: "ask",
		hideDockIcon: true,
		trayIcon: "background_only",
		confirmBeforeQuit: true,
		confirmBeforeClosingTab: true,
		restoreWindowGeometry: true,
		restoreShellTerminals: true,
		restoreScrollback: false,
		restoreScrollbackLines: 1000,
		maxTabNameLength: 25,
		splitTabMode: "separate",
		tabOrderingMode: "grouped-by-type",
		tabCyclingAllTypes: false,
		tabTreeEnabled: false,
		chatMuteIntermediateText: true,
		autoShowPrPopover: true,
		preventSleepWhenBusy: false,
		autoUpdateEnabled: true,
		autoUpdatePluginsEnabled: true,
		deferUpdateRelaunch: true,
		eagerResumeAfterUpdate: true,
		language: "en",
		updateChannel: "stable" as UpdateChannel,
		disabledAgents: [],
		intentTabTitle: true,
		progressTracking: true,
		suggestFollowups: true,
		copyOnSelect: true,
		osc52Clipboard: true,
		osc1337FocusAttention: true,
		showLastPrompt: true,
		bellStyle: "visual",
		globalHotkey: null,
		issueFilter: "assigned",
		prHideDrafts: false,
		prHideConflicting: false,
		prHideCiFailing: false,
		experimentalFeaturesEnabled: false,
		scrollbackReflow: true,
		cursorStyle: "bar" as SettingsStoreState["cursorStyle"],
		terminalRenderer: "webgl",
		linkActivation: "click",
		doubleClickAction: "smart",
		wordSelectionMode: "characters",
		wordSeparators: DEFAULT_WORD_SEPARATORS,
		wordSelectionRegex: "",
		smartSelectionRules: [],
		blockTimestampMode: "modifier",
		showBlockMarks: true,
		showPromptMarks: true,
		blockFoldingEnabled: true,
		indexStrategy: "active_and_switch",
		standbyTimeoutMinutes: 5,
		indexMemoryBudgetMb: settingNumberDefault("app.index_memory_budget_mb"),
		attachmentMaxBytes: settingNumberDefault("app.attachment_max_bytes"),
		webviewUnloadMinutes: settingNumberDefault("app.background.webview_unload_minutes"),
		customLaunchers: [],
		additionalReadableDirs: ["~/.claude/plans"],
		customPtyEnv: [],
		inlineBlameEnabled: true,
		egoExecutable: "",
		egoProfile: "",
		aiChatWorkspace: "",
		diffIgnoreLeadingWhitespace: false,
		diffIgnoreTrailingWhitespace: false,
		diffIgnoreWhitespaceAmount: false,
		diffIgnoreCase: false,
		sessionDiffAutoOpen: "ask",
		sessionDiffTruncateLines: 300,
		indicatorOverrides: [],
		showDiffStats: true,
		showPrBadges: true,
		showGitState: true,
		tabTypeHighlighting: true,
	});

	// Cache of the last loaded config, refreshed on hydrate and each persist.
	// Only read by loadFontFromConfig() now — the save path no longer builds
	// on top of this snapshot (it does a fresh load-modify-save instead).
	let baseConfig: RustAppConfig | null = null;
	let saveTimer: ReturnType<typeof setTimeout> | null = null;
	// Write lock: a save before successful hydrate could overwrite config.json
	// with defaults, wiping the user's settings (observed in the field).
	// No successful hydrate, no persist.
	let hydrated = false;

	// Compare with this window's normalized local state, never a fresh disk load:
	// another window may have changed even fields owned by this same store.
	let savedFields: Record<string, string> = {};
	let savedRender: Partial<SettingsStoreState> = {};
	const [conflicts, setConflicts] = createSignal<string[]>([]);
	let stopRemoteSync: (() => void) | undefined;

	function renderedConfig(config: RustAppConfig): SettingsStoreState {
		const values = {} as SettingsStoreState;
		values.font = validateFont(config.font_family);
		values.fontWeight = config.font_weight || DEFAULTS.fontWeight;
		values.ide = validateIde(config.ide);
		values.defaultFontSize = config.default_font_size || DEFAULTS.fontSize;
		values.shell = config.shell || null;
		values.theme = config.theme || DEFAULT_THEME;
		values.closeBehavior = config.close_behavior ?? "ask";
		values.hideDockIcon = config.background?.hide_dock_icon ?? true;
		values.trayIcon = config.background?.tray_icon ?? "background_only";
		values.confirmBeforeQuit = config.confirm_before_quit ?? true;
		values.confirmBeforeClosingTab = config.confirm_before_closing_tab ?? true;
		values.maxTabNameLength = config.max_tab_name_length || 25;
		values.splitTabMode = config.split_tab_mode === "unified" ? "unified" : "separate";
		const tom = config.tab_ordering_mode;
		values.tabOrderingMode = tom === "terminals-first" || tom === "free" ? tom : "grouped-by-type";
		values.tabCyclingAllTypes = config.tab_cycling_all_types ?? false;
		values.tabTreeEnabled = config.tab_tree_enabled ?? false;
		values.chatMuteIntermediateText = config.chat_mute_intermediate_text ?? true;
		values.autoShowPrPopover = config.auto_show_pr_popover ?? true;
		values.preventSleepWhenBusy = config.prevent_sleep_when_busy ?? false;
		values.autoUpdateEnabled = config.auto_update_enabled ?? true;
		values.autoUpdatePluginsEnabled = config.auto_update_plugins_enabled ?? true;
		values.deferUpdateRelaunch = config.defer_update_relaunch ?? true;
		values.eagerResumeAfterUpdate = config.eager_resume_after_update ?? true;
		values.language = config.language || "en";
		const channel = config.update_channel;
		values.updateChannel = channel === "nightly" || channel === "beta" ? channel : "stable";
		values.disabledAgents = config.disabled_agents ?? [];
		values.intentTabTitle = config.intent_tab_title ?? true;
		values.progressTracking = config.progress_tracking ?? true;
		values.copyOnSelect = config.copy_on_select ?? true;
		values.osc52Clipboard = config.osc52_clipboard ?? true;
		values.osc1337FocusAttention = config.osc1337_focus_attention ?? true;
		values.showLastPrompt = config.show_last_prompt ?? true;
		values.bellStyle = (config.bell_style || "visual") as SettingsStoreState["bellStyle"];
		values.suggestFollowups = config.suggest_followups ?? true;
		values.globalHotkey = config.global_hotkey ?? null;
		values.issueFilter = validateIssueFilter(config.issue_filter || null);
		values.prHideDrafts = config.pr_hide_drafts ?? false;
		values.prHideConflicting = config.pr_hide_conflicting ?? false;
		values.prHideCiFailing = config.pr_hide_ci_failing ?? false;
		values.experimentalFeaturesEnabled = config.experimental_features_enabled ?? false;
		values.scrollbackReflow = config.scrollback_reflow ?? true;
		const cs = config.cursor_style;
		values.cursorStyle = cs === "block" || cs === "underline" ? cs : "bar";
		values.terminalRenderer = validateTerminalRenderer(config.terminal_renderer || null);
		values.linkActivation = validateLinkActivation(config.terminal_link_activation ?? null);
		values.doubleClickAction = validateDoubleClickAction(config.double_click_action ?? null);
		values.wordSelectionMode = validateWordSelectionMode(config.word_selection_mode ?? null);
		values.wordSeparators = config.word_separators ?? DEFAULT_WORD_SEPARATORS;
		values.wordSelectionRegex = config.word_selection_regex ?? "";
		values.smartSelectionRules = (config.smart_selection_rules ?? []).map(ruleFromWire);
		values.blockTimestampMode = resolveBlockTimestampMode(
			config.block_timestamp_mode ?? null,
			config.show_block_timestamps,
		);
		values.showBlockMarks = config.show_block_marks ?? true;
		values.showPromptMarks = config.show_prompt_marks ?? true;
		values.blockFoldingEnabled = config.block_folding_enabled ?? true;
		values.indexStrategy = (config.index_strategy as SettingsStoreState["indexStrategy"]) ?? "active_and_switch";
		values.standbyTimeoutMinutes = config.standby_timeout_minutes ?? 5;
		values.indexMemoryBudgetMb = config.index_memory_budget_mb ?? settingNumberDefault("app.index_memory_budget_mb");
		values.attachmentMaxBytes = config.attachment_max_bytes ?? settingNumberDefault("app.attachment_max_bytes");
		values.webviewUnloadMinutes =
			config.background?.webview_unload_minutes ?? settingNumberDefault("app.background.webview_unload_minutes");
		values.customLaunchers = config.custom_launchers ?? [];
		values.additionalReadableDirs = config.additional_readable_dirs ?? ["~/.claude/plans"];
		// Revalidated here, not just on write — a hand-edited config.json is
		// untrusted input reaching real process environment variables.
		values.customPtyEnv = sanitizeCustomPtyEnv(config.custom_pty_env ?? []);
		values.diffIgnoreLeadingWhitespace = config.diff_ignore_leading_whitespace ?? false;
		values.diffIgnoreTrailingWhitespace = config.diff_ignore_trailing_whitespace ?? false;
		values.diffIgnoreWhitespaceAmount = config.diff_ignore_whitespace_amount ?? false;
		values.diffIgnoreCase = config.diff_ignore_case ?? false;
		// The Rust field is a plain unvalidated String (matches diff_view_mode's
		// own convention) — an unrecognized value on disk falls back to "ask"
		// here, at the frontend's rendering boundary, rather than at load time.
		values.sessionDiffAutoOpen =
			config.session_diff_auto_open === "off" || config.session_diff_auto_open === "auto"
				? config.session_diff_auto_open
				: "ask";
		values.sessionDiffTruncateLines = config.session_diff_truncate_lines ?? 300;
		values.inlineBlameEnabled = config.inline_blame_enabled ?? true;
		values.egoExecutable = config.ego_executable ?? "";
		values.egoProfile = config.ego_profile ?? "";
		values.aiChatWorkspace = config.ai_chat_workspace ?? "";
		// Revalidated here, not just on write — a hand-edited config.json is
		// untrusted input reaching document.documentElement.style (apply.ts).
		values.indicatorOverrides = sanitizeIndicatorOverrides(config.indicator_overrides ?? []);
		values.showDiffStats = config.show_diff_stats ?? true;
		values.showPrBadges = config.show_pr_badges ?? true;
		values.showGitState = config.show_git_state ?? true;
		values.tabTypeHighlighting = config.tab_type_highlighting ?? true;
		values.restoreWindowGeometry = config.restore_window_geometry ?? true;
		values.restoreShellTerminals = config.restore_shell_terminals ?? true;
		values.restoreScrollback = config.restore_scrollback ?? false;
		// `??`, not `||`: a persisted 0 line-cap is a real value, not "unset".
		values.restoreScrollbackLines = config.restore_scrollback_lines ?? 1000;
		return values;
	}

	/** The single mapping from settings state to config fields. Foreign-owned
	 *  services, mcp_server_enabled and global_hotkey stay outside this projection. */
	function ownedFields(values: SettingsStoreState = state) {
		return {
			shell: values.shell,
			font_family: values.font,
			font_size: values.defaultFontSize,
			font_weight: values.fontWeight,
			theme: values.theme,
			ide: values.ide,
			default_font_size: values.defaultFontSize,
			close_behavior: values.closeBehavior,
			background: {
				hide_dock_icon: values.hideDockIcon,
				tray_icon: values.trayIcon,
				webview_unload_minutes: values.webviewUnloadMinutes,
			},
			confirm_before_quit: values.confirmBeforeQuit,
			confirm_before_closing_tab: values.confirmBeforeClosingTab,
			max_tab_name_length: values.maxTabNameLength,
			split_tab_mode: values.splitTabMode,
			tab_ordering_mode: values.tabOrderingMode,
			tab_cycling_all_types: values.tabCyclingAllTypes,
			tab_tree_enabled: values.tabTreeEnabled,
			chat_mute_intermediate_text: values.chatMuteIntermediateText,
			auto_show_pr_popover: values.autoShowPrPopover,
			prevent_sleep_when_busy: values.preventSleepWhenBusy,
			auto_update_enabled: values.autoUpdateEnabled,
			auto_update_plugins_enabled: values.autoUpdatePluginsEnabled,
			defer_update_relaunch: values.deferUpdateRelaunch,
			eager_resume_after_update: values.eagerResumeAfterUpdate,
			language: values.language,
			update_channel: values.updateChannel,
			disabled_agents: [...values.disabledAgents],
			intent_tab_title: values.intentTabTitle,
			progress_tracking: values.progressTracking,
			suggest_followups: values.suggestFollowups,
			copy_on_select: values.copyOnSelect,
			osc52_clipboard: values.osc52Clipboard,
			osc1337_focus_attention: values.osc1337FocusAttention,
			show_last_prompt: values.showLastPrompt,
			bell_style: values.bellStyle,
			issue_filter: values.issueFilter,
			pr_hide_drafts: values.prHideDrafts,
			pr_hide_conflicting: values.prHideConflicting,
			pr_hide_ci_failing: values.prHideCiFailing,
			experimental_features_enabled: values.experimentalFeaturesEnabled,
			scrollback_reflow: values.scrollbackReflow,
			cursor_style: values.cursorStyle,
			terminal_renderer: values.terminalRenderer,
			terminal_link_activation: values.linkActivation,
			double_click_action: values.doubleClickAction,
			word_selection_mode: values.wordSelectionMode,
			word_separators: values.wordSeparators,
			word_selection_regex: values.wordSelectionRegex,
			smart_selection_rules: values.smartSelectionRules.map(ruleToWire),
			block_timestamp_mode: values.blockTimestampMode,
			show_block_marks: values.showBlockMarks,
			show_prompt_marks: values.showPromptMarks,
			block_folding_enabled: values.blockFoldingEnabled,
			index_strategy: values.indexStrategy,
			index_memory_budget_mb: values.indexMemoryBudgetMb,
			attachment_max_bytes: values.attachmentMaxBytes,
			standby_timeout_minutes: values.standbyTimeoutMinutes,
			custom_launchers: [...values.customLaunchers],
			additional_readable_dirs: [...values.additionalReadableDirs],
			custom_pty_env: values.customPtyEnv.map((e) => ({ ...e })),
			diff_ignore_leading_whitespace: values.diffIgnoreLeadingWhitespace,
			diff_ignore_trailing_whitespace: values.diffIgnoreTrailingWhitespace,
			diff_ignore_whitespace_amount: values.diffIgnoreWhitespaceAmount,
			diff_ignore_case: values.diffIgnoreCase,
			session_diff_auto_open: values.sessionDiffAutoOpen,
			session_diff_truncate_lines: values.sessionDiffTruncateLines,
			inline_blame_enabled: values.inlineBlameEnabled,
			ego_executable: values.egoExecutable,
			ego_profile: values.egoProfile,
			ai_chat_workspace: values.aiChatWorkspace,
			indicator_overrides: values.indicatorOverrides.map((o) => ({ ...o })),
			show_diff_stats: values.showDiffStats,
			show_pr_badges: values.showPrBadges,
			show_git_state: values.showGitState,
			tab_type_highlighting: values.tabTypeHighlighting,
			restore_window_geometry: values.restoreWindowGeometry,
			restore_shell_terminals: values.restoreShellTerminals,
			restore_scrollback: values.restoreScrollback,
			restore_scrollback_lines: values.restoreScrollbackLines,
		} satisfies Partial<RustAppConfig>;
	}

	/** Immutable values also detect edits to nested launcher arguments. */
	function snapshotFields(fields: ReturnType<typeof ownedFields>): Record<string, string> {
		return Object.fromEntries(Object.entries(fields).map(([key, value]) => [key, JSON.stringify(value)]));
	}

	/** No migration, save, setter side effects or dirty-state reset on a remote read. */
	function applyConfig(config: RustAppConfig, preserveDirty: boolean): void {
		const incoming = renderedConfig(config);
		const conflicting: string[] = [];
		const patch: Partial<SettingsStoreState> = { ...incoming };
		for (const key of Object.keys(incoming) as (keyof SettingsStoreState)[]) {
			const local = JSON.stringify(state[key]);
			const base = JSON.stringify(savedRender[key]);
			const next = JSON.stringify(incoming[key]);
			if (preserveDirty && local !== base) {
				delete patch[key];
				if (next !== base && next !== local) conflicting.push(key);
			}
		}
		setState(patch);
		baseConfig = config;
		savedRender = JSON.parse(JSON.stringify(incoming));
		savedFields = snapshotFields(ownedFields(incoming));
		setConflicts(conflicting);
		setLocale(state.language);
		if (preserveDirty && saveTimer) {
			clearTimeout(saveTimer);
			saveTimer = null;
			if (conflicting.length === 0 && JSON.stringify(snapshotFields(ownedFields())) !== JSON.stringify(savedFields))
				save();
		}
		if (conflicting.length > 0) {
			toastsStore.add(
				"Settings changed elsewhere",
				"Your unsaved edits were kept. Review them before saving again.",
				"warn",
			);
		}
	}

	/** Apply only local edits to a fresh document. The backend derives its delta
	 *  from this base/config pair while holding the cross-process file lock. */
	async function persist(): Promise<void> {
		try {
			let snapshot = savedFields;
			const config = await updateAppConfig<RustAppConfig>((fresh) => {
				const fields = ownedFields();
				snapshot = snapshotFields(fields);
				Object.assign(
					fresh,
					Object.fromEntries(
						Object.entries(fields)
							.filter(([key]) => snapshot[key] !== savedFields[key])
							.map(([key, value]) => [
								key,
								key === "background"
									? {
											...fresh.background,
											...Object.fromEntries(
												Object.entries(fields.background).filter(
													([name, next]) =>
														JSON.stringify(next) !== JSON.stringify(JSON.parse(savedFields.background ?? "{}")[name]),
												),
											),
										}
									: value,
							]),
					),
				);
			});
			savedFields = snapshot; // failed saves keep their edits pending for a later attempt
			baseConfig = config; // keep the cache (loadFontFromConfig) current
			savedRender = renderedConfig(config);
		} catch (err) {
			appLogger.error("config", "Failed to save config", err);
		}
	}

	/** Debounced save — coalesces rapid setting changes into a single persist */
	function save(): void {
		if (!hydrated) {
			appLogger.error(
				"config",
				"Refusing to persist settings: store not hydrated — would clobber config.json with defaults",
			);
			return;
		}
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			saveTimer = null;
			void persist();
		}, SAVE_DEBOUNCE_MS);
	}

	/** Set (or replace) one field of one indicator's override, appending a new
	 *  override row if this id has none yet. Shared by setIndicatorColor/Icon/Animation. */
	function upsertIndicatorField(id: string, field: "color" | "icon" | "animation", value: string): void {
		const existing = state.indicatorOverrides.find((o) => o.id === id);
		if (existing) {
			setState("indicatorOverrides", (o) => o.id === id, field, value);
		} else {
			setState("indicatorOverrides", state.indicatorOverrides.length, { id, [field]: value });
		}
		save();
	}

	const actions = {
		conflicts,
		/** App-lifetime, idempotent subscription; teardown also invalidates pending reads. */
		startRemoteSync(): () => void {
			if (!stopRemoteSync)
				stopRemoteSync = settingsRefresh.register({
					section: "app",
					read: () => runSerializedConfigWrite(() => invoke<RustAppConfig>("load_config")),
					apply: (config) => {
						if (hydrated) applyConfig(config, true);
					},
				});
			return () => {
				stopRemoteSync?.();
				stopRemoteSync = undefined;
			};
		},
		/** Load settings from Rust config; migrate from localStorage on first run */
		async hydrate(options?: { notifyOnError?: boolean }): Promise<boolean> {
			try {
				// One-time migration from localStorage
				const legacyIde = localStorage.getItem(LEGACY_KEYS.IDE);
				if (legacyIde) {
					try {
						await updateAppConfig<RustAppConfig>((config) => {
							config.ide = legacyIde;
						});
					} catch {
						/* ignore migration failure */
					}
					localStorage.removeItem(LEGACY_KEYS.IDE);
				}
				// Clean up legacy session key
				localStorage.removeItem(LEGACY_KEYS.SESSION);

				const config = await invoke<RustAppConfig>("load_config");
				baseConfig = config;
				applyConfig(config, false);
				savedFields = snapshotFields(ownedFields());
				hydrated = true;
				return true;
			} catch (err) {
				appLogger.error("config", "Failed to hydrate settings — persistence disabled for this session", err);
				if (options?.notifyOnError !== false)
					toastsStore.add(
						"Settings failed to load",
						"Running on defaults; changes will NOT be saved. Check logs and restart.",
						"error",
					);
				return false;
			}
		},

		/** Set IDE preference */
		setIde(ide: IdeType): void {
			setState("ide", ide);
			save();
		},

		/** Set font preference */
		setFont(font: FontType): void {
			setState("font", font);
			save();
		},

		/** Set terminal font weight */
		setFontWeight(weight: number): void {
			setState("fontWeight", Math.max(100, Math.min(900, Math.round(weight / 100) * 100)));
			save();
		},

		/** Set default font size */
		setDefaultFontSize(size: number): void {
			setState("defaultFontSize", Math.max(8, Math.min(32, size)));
			save();
		},

		/** Set custom shell override (null = use system default) */
		setShell(shell: string | null): void {
			setState("shell", shell?.trim() || null);
			save();
		},

		/** Set terminal theme */
		setTheme(theme: string): void {
			setState("theme", theme);
			save();
		},

		/** Set the main-window close preference. */
		setCloseBehavior(behavior: CloseBehavior) {
			setState("closeBehavior", behavior);
			save();
		},

		setTrayIcon(value: TrayIconPreference) {
			setState("trayIcon", value);
			save();
		},

		setHideDockIcon(enabled: boolean) {
			setState("hideDockIcon", enabled);
			save();
		},

		setConfirmBeforeQuit(enabled: boolean): void {
			setState("confirmBeforeQuit", enabled);
			save();
		},

		/** Set confirm-before-closing-tab preference */
		setConfirmBeforeClosingTab(enabled: boolean): void {
			setState("confirmBeforeClosingTab", enabled);
			save();
		},

		/** Set whether the main window's size/position is restored on launch */
		setRestoreWindowGeometry(enabled: boolean): void {
			setState("restoreWindowGeometry", enabled);
			save();
		},

		/** Set whether plain shell tabs (not just agent tabs) are restored on launch */
		setRestoreShellTerminals(enabled: boolean): void {
			setState("restoreShellTerminals", enabled);
			save();
		},

		/** Set whether terminal scrollback is persisted to disk and replayed on restore */
		setRestoreScrollback(enabled: boolean): void {
			setState("restoreScrollback", enabled);
			save();
		},

		/** Set the max scrollback lines persisted per terminal */
		setRestoreScrollbackLines(lines: number): void {
			setState("restoreScrollbackLines", Math.max(0, Math.round(lines)));
			save();
		},

		/** Set split tab mode preference */
		setSplitTabMode(mode: SplitTabMode): void {
			setState("splitTabMode", mode);
			save();
		},

		setTabOrderingMode(mode: TabOrderingMode): void {
			setState("tabOrderingMode", mode);
			save();
		},

		setTabCyclingAllTypes(enabled: boolean): void {
			setState("tabCyclingAllTypes", enabled);
			save();
		},

		setTabTreeEnabled(enabled: boolean): void {
			setState("tabTreeEnabled", enabled);
			save();
		},

		setChatMuteIntermediateText(enabled: boolean): void {
			setState("chatMuteIntermediateText", enabled);
			save();
		},

		/** Set auto-show PR popover preference */
		setAutoShowPrPopover(enabled: boolean): void {
			setState("autoShowPrPopover", enabled);
			save();
		},

		/** Set prevent-sleep-when-busy preference */
		setPreventSleepWhenBusy(enabled: boolean): void {
			setState("preventSleepWhenBusy", enabled);
			save();
		},

		setStandbyTimeoutMinutes(minutes: number): void {
			setState("standbyTimeoutMinutes", Math.max(0, Math.min(60, minutes)));
			save();
		},

		setIndexMemoryBudgetMb(value: number): void {
			if (!Number.isSafeInteger(value) || value < 0) return;
			setState("indexMemoryBudgetMb", value);
			save();
		},
		setAttachmentMaxBytes(value: number): void {
			if (!Number.isSafeInteger(value) || value < 1) return;
			setState("attachmentMaxBytes", value);
			save();
		},
		setWebviewUnloadMinutes(value: number): void {
			if (!Number.isSafeInteger(value) || value < 0) return;
			setState("webviewUnloadMinutes", value);
			save();
		},

		setIndexStrategy(strategy: SettingsStoreState["indexStrategy"]): void {
			setState("indexStrategy", strategy);
			save();
		},

		/** Set auto-update-enabled preference */
		setAutoUpdateEnabled(enabled: boolean): void {
			setState("autoUpdateEnabled", enabled);
			save();
		},

		/** Set auto-update-plugins-enabled preference */
		setAutoUpdatePluginsEnabled(enabled: boolean): void {
			setState("autoUpdatePluginsEnabled", enabled);
			save();
		},

		/** Set defer-update-relaunch preference */
		setDeferUpdateRelaunch(enabled: boolean): void {
			setState("deferUpdateRelaunch", enabled);
			save();
		},

		/** Set eager-resume-after-update preference */
		setEagerResumeAfterUpdate(enabled: boolean): void {
			setState("eagerResumeAfterUpdate", enabled);
			save();
		},

		/** Set update channel preference */
		setUpdateChannel(channel: UpdateChannel): void {
			setState("updateChannel", channel);
			save();
		},

		/** Set UI language */
		setLanguage(language: string): void {
			setState("language", language);
			setLocale(language);
			save();
		},

		/** Set max tab name length */
		setMaxTabNameLength(length: number): void {
			setState("maxTabNameLength", Math.max(10, Math.min(60, length)));
			save();
		},

		/** Toggle an agent's enabled/disabled state */
		toggleAgent(agentType: string): void {
			const isDisabled = state.disabledAgents.includes(agentType);
			setState(
				"disabledAgents",
				isDisabled ? state.disabledAgents.filter((a) => a !== agentType) : [...state.disabledAgents, agentType],
			);
			save();
		},

		/** Check if an agent type is enabled */
		isAgentEnabled(agentType: string): boolean {
			return !state.disabledAgents.includes(agentType);
		},

		/** Replace the full list of custom launchers (add/edit/remove all go through here) */
		setCustomLaunchers(launchers: CustomLauncher[]): void {
			setState("customLaunchers", launchers);
			save();
		},

		/** Replace the full list of additional HTTP-readable directories (add/remove go through here) */
		setAdditionalReadableDirs(dirs: string[]): void {
			setState("additionalReadableDirs", dirs);
			save();
		},

		/** Replace the full list of custom PTY env vars (add/edit/remove all go through here) */
		setCustomPtyEnv(entries: CustomEnvVarEntry[]): void {
			setState("customPtyEnv", entries);
			save();
		},

		/** Set intent-as-tab-title preference */
		setIntentTabTitle(enabled: boolean): void {
			setState("intentTabTitle", enabled);
			save();
		},

		/** Turn Progress collection on or off for every agent. */
		setProgressTracking(enabled: boolean): void {
			setState("progressTracking", enabled);
			save();
		},

		/** Set suggest-followups preference */
		setSuggestFollowups(enabled: boolean): void {
			setState("suggestFollowups", enabled);
			save();
		},

		/** Set copy-on-select preference */
		setCopyOnSelect(enabled: boolean): void {
			setState("copyOnSelect", enabled);
			save();
		},

		/** Enable/disable honoring OSC 52 clipboard-write sequences from terminal output */
		setOsc52Clipboard(enabled: boolean): void {
			setState("osc52Clipboard", enabled);
			save();
		},

		/** Enable/disable honoring OSC 1337 StealFocus/RequestAttention sequences from terminal output */
		setOsc1337FocusAttention(enabled: boolean): void {
			setState("osc1337FocusAttention", enabled);
			save();
		},

		setShowLastPrompt(enabled: boolean): void {
			setState("showLastPrompt", enabled);
			save();
		},

		/** Set issue filter mode */
		setIssueFilter(filter: IssueFilterMode): void {
			setState("issueFilter", filter);
			save();
		},

		setPrHideDrafts(v: boolean): void {
			setState("prHideDrafts", v);
			save();
		},

		setPrHideConflicting(v: boolean): void {
			setState("prHideConflicting", v);
			save();
		},

		setPrHideCiFailing(v: boolean): void {
			setState("prHideCiFailing", v);
			save();
		},

		/** Set terminal bell style */
		setBellStyle(style: SettingsStoreState["bellStyle"]): void {
			setState("bellStyle", style);
			save();
		},

		setExperimentalFeaturesEnabled(enabled: boolean): void {
			setState("experimentalFeaturesEnabled", enabled);
			save();
		},

		setScrollbackReflow(enabled: boolean): void {
			setState("scrollbackReflow", enabled);
			save();
		},

		/** When each command block's relative start time is painted: never, while Ctrl+Cmd is held, or always */
		setBlockTimestampMode(mode: BlockTimestampMode): void {
			setState("blockTimestampMode", mode);
			save();
		},

		/** Allow collapsing a command block's output with the fold shortcut */
		setBlockFoldingEnabled(enabled: boolean): void {
			setState("blockFoldingEnabled", enabled);
			save();
		},

		/** Set the ego binary the AI Chat panel launches. Empty disables ACP. */
		setEgoExecutable(path: string): void {
			setState("egoExecutable", path.trim());
			save();
		},

		/** Keep the profile as one unambiguous argument, or leave it unset. */
		setEgoProfile(name: string): boolean {
			if (
				name !== "" &&
				(name.startsWith("-") || /\s|[\x00-\x1f\x7f\x85]/u.test(name) || new TextEncoder().encode(name).length > 64)
			) {
				return false;
			}
			setState("egoProfile", name);
			save();
			return true;
		},

		/** Set the folder AI Chat runs in. Empty means the host home directory;
		 *  anything else must be absolute, as the backend requires. */
		setAiChatWorkspace(path: string): boolean {
			const value = path.trim();
			if (value !== "" && !/^([/\\]|[A-Za-z]:[/\\])/.test(value)) return false;
			setState("aiChatWorkspace", value);
			save();
			return true;
		},

		/** Toggle GitLens-style inline git blame on the editor's active line */
		setInlineBlameEnabled(enabled: boolean): void {
			setState("inlineBlameEnabled", enabled);
			save();
		},

		setDiffIgnoreLeadingWhitespace(enabled: boolean): void {
			setState("diffIgnoreLeadingWhitespace", enabled);
			save();
		},

		setDiffIgnoreTrailingWhitespace(enabled: boolean): void {
			setState("diffIgnoreTrailingWhitespace", enabled);
			save();
		},

		setDiffIgnoreWhitespaceAmount(enabled: boolean): void {
			setState("diffIgnoreWhitespaceAmount", enabled);
			save();
		},

		setDiffIgnoreCase(enabled: boolean): void {
			setState("diffIgnoreCase", enabled);
			save();
		},

		setSessionDiffAutoOpen(mode: "off" | "ask" | "auto"): void {
			setState("sessionDiffAutoOpen", mode);
			save();
		},

		setSessionDiffTruncateLines(lines: number): void {
			setState("sessionDiffTruncateLines", Math.max(0, Math.trunc(lines)));
			save();
		},

		/** Set (or replace) one indicator's color override. Silently rejects an
		 *  unsafe value — see indicators/validate.ts's isSafeIndicatorColor. */
		setIndicatorColor(id: string, color: string): void {
			if (!isSafeIndicatorColor(color)) {
				appLogger.warn("config", `Rejected unsafe indicator color for "${id}"`, { color });
				return;
			}
			upsertIndicatorField(id, "color", color);
		},

		/** Set (or replace) one indicator's icon override. Silently rejects an
		 *  id this build's icon set doesn't know about. */
		setIndicatorIcon(id: string, iconId: string): void {
			if (!isSafeIndicatorIconId(iconId)) {
				appLogger.warn("config", `Rejected unknown indicator icon id for "${id}"`, { iconId });
				return;
			}
			upsertIndicatorField(id, "icon", iconId);
		},

		/** Set (or replace) one indicator's animation override. Silently rejects
		 *  an id this build's animation set doesn't know about. */
		setIndicatorAnimation(id: string, animationId: string): void {
			if (!isSafeIndicatorAnimationId(animationId)) {
				appLogger.warn("config", `Rejected unknown indicator animation id for "${id}"`, { animationId });
				return;
			}
			upsertIndicatorField(id, "animation", animationId);
		},

		/** Clear every field of one indicator's override, reverting it to the
		 *  registry default. */
		clearIndicatorOverride(id: string): void {
			setState(
				"indicatorOverrides",
				state.indicatorOverrides.filter((o) => o.id !== id),
			);
			save();
		},

		/** Clear one field of one indicator's override, leaving any other
		 *  overridden fields on that same indicator untouched — unlike
		 *  `clearIndicatorOverride`, which drops the whole row. Needed once
		 *  color/icon/animation are edited from one combined dialog: clearing
		 *  just the color (e.g. via "No color") must not also drop an icon or
		 *  animation override on the same indicator. Drops the row entirely
		 *  once no field remains, matching what `sanitizeIndicatorOverrides`
		 *  already does to a bare `{ id }` row on hydrate. */
		clearIndicatorField(id: string, field: "color" | "icon" | "animation"): void {
			const existing = state.indicatorOverrides.find((o) => o.id === id);
			if (!existing) return;
			const updated: IndicatorOverride = { ...existing };
			delete updated[field];
			const stillOverridden =
				updated.color !== undefined || updated.icon !== undefined || updated.animation !== undefined;
			setState(
				"indicatorOverrides",
				stillOverridden
					? state.indicatorOverrides.map((o) => (o.id === id ? updated : o))
					: state.indicatorOverrides.filter((o) => o.id !== id),
			);
			save();
		},

		/** Clear every indicator override — the legend's "Reset all indicators" button. */
		resetAllIndicators(): void {
			setState("indicatorOverrides", []);
			save();
		},

		/** Show/hide the sidebar branch row's diff stat badge (+N/-N). */
		setShowDiffStats(enabled: boolean): void {
			setState("showDiffStats", enabled);
			save();
		},

		/** Show/hide the sidebar PR status badge. */
		setShowPrBadges(enabled: boolean): void {
			setState("showPrBadges", enabled);
			save();
		},

		/** Show/hide git repo status indicators: the sidebar's per-worktree
		 *  rebase/merge/cherry-pick/revert/bisect badge and the Changes tab's
		 *  conflicts banner. */
		setShowGitState(enabled: boolean): void {
			setState("showGitState", enabled);
			save();
		},

		/** Tint tab backgrounds/borders by type. Off neutralizes the tint but
		 *  keeps each type's icon color (see useAppearanceSync.ts). */
		setTabTypeHighlighting(enabled: boolean): void {
			setState("tabTypeHighlighting", enabled);
			save();
		},

		setCursorStyle(style: SettingsStoreState["cursorStyle"]): void {
			setState("cursorStyle", style);
			save();
		},

		setTerminalRenderer(renderer: TerminalRenderer): void {
			setState("terminalRenderer", renderer);
			save();
		},

		/** How terminal links activate: plain click, Cmd/Ctrl+click, or context menu only */
		setLinkActivation(mode: LinkActivation): void {
			setState("linkActivation", mode);
			save();
		},

		setDoubleClickAction(action: DoubleClickAction): void {
			setState("doubleClickAction", action);
			save();
		},

		setWordSelectionMode(mode: WordSelectionMode): void {
			setState("wordSelectionMode", mode);
			save();
		},

		setWordSeparators(separators: string): void {
			setState("wordSeparators", separators);
			save();
		},

		setWordSelectionRegex(regex: string): void {
			setState("wordSelectionRegex", regex);
			save();
		},

		/** Replace the full list of smart-selection rules (add/edit/remove/reorder
		 *  all go through here) — same whole-array-replacement pattern as
		 *  `setCustomLaunchers`. */
		setSmartSelectionRules(rules: SmartSelectionRule[]): void {
			setState("smartSelectionRules", rules);
			save();
		},

		/** Draw command-block boundary ticks on the terminal scrollbar */
		setShowBlockMarks(enabled: boolean): void {
			setState("showBlockMarks", enabled);
			save();
		},

		/** Draw a scrollbar tick for each line where the user submitted a prompt */
		setShowPromptMarks(enabled: boolean): void {
			setState("showPromptMarks", enabled);
			save();
		},

		/** Re-apply font from the last loaded config (no IPC — uses hydrate cache) */
		loadFontFromConfig(): void {
			if (baseConfig) {
				setState("font", validateFont(baseConfig.font_family));
			}
		},

		/** Set global OS-level hotkey (or clear with null) */
		async setGlobalHotkey(combo: string | null): Promise<void> {
			const prevValue = state.globalHotkey;
			setState("globalHotkey", combo);
			try {
				await runSerializedConfigWrite(() => invoke("set_global_hotkey", { combo }));
			} catch (err) {
				appLogger.error("config", "Failed to set global hotkey", err);
				setState("globalHotkey", prevValue);
				throw err;
			}
		},

		/** Get CSS font family string */
		getFontFamily(): string {
			return FONT_FAMILIES[state.font] || FONT_FAMILIES[DEFAULTS.font];
		},

		/** Get IDE display name */
		getIdeName(): string {
			return IDE_NAMES[state.ide] || IDE_NAMES[DEFAULTS.ide];
		},

		/** Whether the AI Chat panel is offered at all.
		 *
		 * Its own `ai_chat_enabled` sub-flag went with the embedded engine
		 * (#784-0aec) — there is nothing left for a second toggle to switch on.
		 * The panel is a shell until ego drives it (#785-58ca), so it stays behind
		 * the experimental master toggle, which is off by default. */
		isAiChatEnabled(): boolean {
			return state.experimentalFeaturesEnabled;
		},

		/** Whether an ego binary is named at all. Rust refuses every connect while
		 *  it is not, so the panel says so instead of launching nothing. */
		isAcpConfigured(): boolean {
			return isAcpConfigured(state.egoExecutable);
		},
	};

	return {
		state,
		...actions,
		_testCancelPendingSave(): void {
			if (saveTimer) {
				clearTimeout(saveTimer);
				saveTimer = null;
			}
		},
	};
}

export const settingsStore = createSettingsStore();

// Debug registry — expose app settings for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("settings", () => {
	const s = settingsStore.state;
	return {
		ide: s.ide,
		font: s.font,
		fontWeight: s.fontWeight,
		defaultFontSize: s.defaultFontSize,
		shell: s.shell,
		theme: s.theme,
		language: s.language,
		splitTabMode: s.splitTabMode,
		tabOrderingMode: s.tabOrderingMode,
		tabCyclingAllTypes: s.tabCyclingAllTypes,
		tabTreeEnabled: s.tabTreeEnabled,
		chatMuteIntermediateText: s.chatMuteIntermediateText,
		bellStyle: s.bellStyle,
		updateChannel: s.updateChannel,
		intentTabTitle: s.intentTabTitle,
		progressTracking: s.progressTracking,
		suggestFollowups: s.suggestFollowups,
		copyOnSelect: s.copyOnSelect,
		autoUpdateEnabled: s.autoUpdateEnabled,
		preventSleepWhenBusy: s.preventSleepWhenBusy,
		issueFilter: s.issueFilter,
		terminalRenderer: s.terminalRenderer,
		linkActivation: s.linkActivation,
		doubleClickAction: s.doubleClickAction,
		wordSelectionMode: s.wordSelectionMode,
		smartSelectionRuleCount: s.smartSelectionRules.length,
		indicatorOverrideCount: s.indicatorOverrides.length,
		showDiffStats: s.showDiffStats,
		showPrBadges: s.showPrBadges,
		showGitState: s.showGitState,
		tabTypeHighlighting: s.tabTypeHighlighting,
	};
});
