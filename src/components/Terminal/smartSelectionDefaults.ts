import type { SmartSelectionRule } from "./smartSelectionTypes";

/**
 * Default smart-selection rules: iTerm2's built-in set (translated verbatim
 * from `plists/SmartSelectionRules.plist` in `gnachman/iTerm2`, POSIX
 * character classes like `[[:letter:]]` swapped for JS-safe `A-Za-z0-9`
 * equivalents) plus TUICommander-relevant additions for a dev/agent terminal
 * (git SHA, `file:line:col`, semver, IPv4/IPv6, UUID, issue key, `#NNN`).
 *
 * One deliberate deviation: iTerm2's HTTP URL rule uses a negative
 * lookbehind (`(?<!\()`) to avoid swallowing a leading paren. WKWebView on
 * older macOS lacks lookbehind support, and `compileRules` would silently
 * drop the whole rule if `new RegExp` throws — so this ships a
 * lookbehind-free variant. Users who want the paren-exclusion behavior can
 * add it back themselves; the rule editor validates regexes at save time.
 *
 * `smart_selection_rules` in config.json is empty by default and this list
 * is what the engine falls back to in that case — see
 * `resolveSmartSelectionRules` — so this is the ONLY place the default set
 * is defined.
 */
export const DEFAULT_SMART_SELECTION_RULES: SmartSelectionRule[] = [
	{
		id: "iterm-word",
		name: "Word bounded by whitespace",
		regex: "\\S+",
		precision: "low",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "iterm-cpp-namespace",
		name: "C++ namespace::identifier",
		regex: "([a-zA-Z0-9_]+::)+[a-zA-Z0-9_]+",
		precision: "normal",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "iterm-path",
		name: "Path",
		regex: "~?/?([A-Za-z0-9._-]+/+)+[A-Za-z0-9._-]+/?",
		precision: "normal",
		enabled: true,
		actions: [
			{ kind: "open_file", title: "Open", parameter: "\\0", isDefault: true },
			{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false },
		],
	},
	{
		id: "iterm-quoted-string",
		name: "Quoted string",
		regex: '@?"(?:[^"\\\\]|\\\\.)*"',
		precision: "normal",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "iterm-java-python-include",
		name: "Java/Python include path",
		regex: "([A-Za-z0-9._]+\\.)+[A-Za-z0-9._]+",
		precision: "normal",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "iterm-mailto",
		name: "mailto URL",
		regex: "\\bmailto:([a-z0-9A-Z_]+@)?([a-zA-Z0-9-]+\\.)*[a-zA-Z0-9-]+\\b",
		precision: "normal",
		enabled: true,
		actions: [
			{ kind: "open_url", title: "Open", parameter: "\\0", isDefault: true },
			{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false },
		],
	},
	{
		id: "iterm-objc-selector",
		name: "Objective-C selector",
		regex: "@selector\\([^)]+\\)",
		precision: "high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "iterm-email",
		name: "Email address",
		regex: "\\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,4}\\b",
		precision: "high",
		enabled: true,
		actions: [
			{ kind: "open_url", title: "Open", parameter: "mailto:\\0", isDefault: true },
			{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false },
		],
	},
	{
		id: "iterm-http-url",
		name: "HTTP URL",
		regex:
			"https?://([a-z0-9A-Z]+(:[a-zA-Z0-9]+)?@)?([a-z0-9A-Z][-a-z0-9A-Z]*\\.)*[A-Za-z][-A-Za-z]*(:[0-9]+)?(/[a-zA-Z0-9;/.\\-_+%?&@=#~()]*)?",
		precision: "very_high",
		enabled: true,
		actions: [
			{ kind: "open_url", title: "Open", parameter: "\\0", isDefault: true },
			{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false },
		],
	},
	{
		id: "iterm-ssh-url",
		name: "SSH URL",
		regex: "\\bssh:([a-z0-9A-Z_]+@)?([a-zA-Z0-9-]+\\.)*[a-zA-Z0-9-]+\\b",
		precision: "very_high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "iterm-telnet-url",
		name: "Telnet URL",
		regex: "\\btelnet:([a-z0-9A-Z_]+@)?([a-zA-Z0-9-]+\\.)*[a-zA-Z0-9-]+\\b",
		precision: "very_high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	// --- Dev/agent-terminal extras ---
	{
		id: "dev-git-sha",
		name: "Git commit SHA",
		regex: "\\b[0-9a-f]{7,40}\\b",
		precision: "high",
		enabled: true,
		actions: [
			{ kind: "run_command", title: "Show commit", parameter: "git show \\0", isDefault: true },
			{ kind: "copy", title: "Copy SHA", parameter: "\\0", isDefault: false },
		],
	},
	{
		id: "dev-file-line-col",
		name: "file:line:col",
		regex: "[A-Za-z0-9_./-]+\\.[A-Za-z0-9]+:[0-9]+(:[0-9]+)?",
		precision: "high",
		enabled: true,
		actions: [
			{ kind: "open_file", title: "Open", parameter: "\\0", isDefault: true },
			{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false },
		],
	},
	{
		id: "dev-semver",
		name: "Semantic version",
		regex: "\\bv?[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?(\\+[0-9A-Za-z.-]+)?\\b",
		precision: "high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "dev-ipv4",
		name: "IPv4 address",
		regex: "\\b(?:[0-9]{1,3}\\.){3}[0-9]{1,3}\\b",
		precision: "high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "dev-ipv6",
		name: "IPv6 address",
		// Covers both fully-expanded and "::"-compressed forms (e.g. "fe80::1",
		// "::1"). The bare-trailing-"::" alternative (nothing after the
		// compression, e.g. "fe80::" alone) is deliberately LAST: unlike a
		// backtracking full-string match, `exec`'s substring search takes the
		// first alternative that succeeds at a given position, so a shorter
		// "ends at ::" alternative ahead of "::" + trailing group" would win
		// and truncate a real address like "fe80::1" to "fe80::".
		regex:
			"([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7})|([0-9a-fA-F]{1,4}:){1,7}:|::",
		precision: "normal",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "dev-uuid",
		name: "UUID",
		regex: "\\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\\b",
		precision: "very_high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "dev-issue-key",
		name: "Issue key (e.g. ABC-123)",
		regex: "\\b[A-Z][A-Z0-9]+-[0-9]+\\b",
		precision: "normal",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
	{
		id: "dev-issue-ref",
		name: "Issue reference (#NNN)",
		regex: "#[0-9]+\\b",
		precision: "low",
		// Off by default: too generic on its own (collides with hex colors like
		// `#fff`, markdown headers, etc.) — precision alone can't save it since
		// low precision means it only wins when nothing more specific matches.
		enabled: false,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	},
];

/** Rules to actually match against: user-defined rules if any exist,
 *  otherwise the shipped defaults. An empty `smart_selection_rules` in
 *  config.json means "use the defaults" — see `smartSelectionTypes.ts`'s
 *  doc comment and `docs/backend/config.md`. */
export function resolveSmartSelectionRules(userRules: SmartSelectionRule[]): SmartSelectionRule[] {
	return userRules.length > 0 ? userRules : DEFAULT_SMART_SELECTION_RULES;
}
