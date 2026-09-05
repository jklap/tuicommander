/** Static extraction of the settings a tab renders, straight from its source.
 *
 * The search index in `settingsSearchIndex.ts` is a committed file, so it could
 * drift from the tabs it describes. This module re-derives the same data from
 * the JSX and the drift test compares the two — that is the whole anti-drift
 * mechanism (see the design note in `settingsSearchIndex.ts`).
 *
 * Extraction rule, deliberately mechanical:
 *   - every `<h3>` opens a section,
 *   - every `label=` prop and every `<label>` element is a setting inside the
 *     section opened by the nearest preceding `<h3>`,
 *   - text is read from `t("key", "Default")`, from a `{"literal"}` or from a
 *     leading plain-text run; anything else is dynamic and is only counted.
 */

export interface ExtractedText {
	/** i18n key, when the text came from a `t()` call */
	key?: string;
	/** Default (English) text */
	text: string;
}

export interface ExtractedTab {
	sections: ExtractedText[];
	/** Settings, each tagged with the section heading text it sits under */
	settings: (ExtractedText & { section: string })[];
	/** Occurrences whose text is computed at runtime and cannot be indexed */
	dynamic: number;
}

/** Index of the `>` that closes the open tag starting at `from`, brace-aware.
 *
 * `<label onClick={(e) => e.stopPropagation()}>` has a `>` inside an attribute
 * expression; a naive scan stops there and reads the arrow body as the label. */
function endOfOpenTag(src: string, from: number): number {
	let depth = 0;
	for (let i = from; i < src.length; i++) {
		const c = src[i];
		if (c === "{") depth++;
		else if (c === "}") depth--;
		else if (c === ">" && depth === 0) return i;
	}
	return -1;
}

const T_CALL = /^\{\s*t\(\s*"([^"]*)"\s*,\s*"((?:[^"\\]|\\.)*)"/;
const BRACED_LITERAL = /^\{\s*"((?:[^"\\]|\\.)*)"\s*\}/;
const BARE_LITERAL = /^"((?:[^"\\]|\\.)*)"/;
const PLAIN = /^([^<{}]+)/;

/** Read the statically-known text out of `t("k","V")`, `{"V"}` or `"V"`. */
function staticExpression(trimmed: string): ExtractedText | null {
	const call = trimmed.match(T_CALL);
	if (call) return { key: call[1], text: unescapeJsx(call[2]) };
	const braced = trimmed.match(BRACED_LITERAL);
	if (braced) return { text: unescapeJsx(braced[1]) };
	const bare = trimmed.match(BARE_LITERAL);
	if (bare) return { text: unescapeJsx(bare[1]) };
	return null;
}

/** Read the first statically-known text out of a JSX child run, or null.
 *
 * Children may be plain text (`<h3>Parameters</h3>`); a prop value may not —
 * accepting a plain run there would read arbitrary code as a label. */
export function staticText(inner: string, allowPlain: boolean): ExtractedText | null {
	const trimmed = inner.trim();
	const expr = staticExpression(trimmed);
	if (expr) return expr;
	if (!allowPlain) return null;
	const plain = trimmed.match(PLAIN);
	if (plain) {
		const text = plain[1].replace(/\s+/g, " ").trim();
		if (text) return { text };
	}
	return null;
}

function unescapeJsx(s: string): string {
	return s.replace(/\\"/g, '"').replace(/\\n/g, " ").replace(/\\\\/g, "\\");
}

/** Element occurrences (`<h3>`, `<label>`) and `label=` props, in source order. */
function* occurrences(src: string): Generator<{ kind: "h3" | "label"; inner: string; isProp: boolean }> {
	const re = /<(h3|label)\b|\blabel=/g;
	let m: RegExpExecArray | null;
	while ((m = re.exec(src)) !== null) {
		if (m[1]) {
			const open = endOfOpenTag(src, m.index);
			if (open < 0) continue;
			const close = src.indexOf(`</${m[1]}>`, open);
			if (close < 0) continue;
			yield { kind: m[1] as "h3" | "label", inner: src.slice(open + 1, close), isProp: false };
			re.lastIndex = open + 1;
		} else {
			yield { kind: "label", inner: src.slice(m.index + "label=".length), isProp: true };
		}
	}
}

export function extractTab(src: string): ExtractedTab {
	const out: ExtractedTab = { sections: [], settings: [], dynamic: 0 };
	let section = "";
	for (const occ of occurrences(src)) {
		const text = staticText(occ.inner, !occ.isProp);
		if (!text) {
			out.dynamic++;
			continue;
		}
		if (occ.kind === "h3") {
			section = text.text;
			out.sections.push(text);
		} else {
			out.settings.push({ ...text, section });
		}
	}
	return out;
}

/** Nav keys `SettingsPanel` actually renders a tab body for.
 *
 * Read from the `<Show when={activeTab() === "…"}>` guards rather than from the
 * nav list: the guards are what decides whether a search result can open
 * anything, so a tab added there but missing from the index is the exact drift
 * the index test must catch. Repo tabs are keyed on `activeRepoPath()` and do
 * not appear here — they are intentionally unindexed. */
export function extractRenderedTabKeys(src: string): string[] {
	const keys = new Set<string>();
	for (const m of src.matchAll(/activeTab\(\)\s*===\s*"([^"]+)"/g)) keys.add(m[1]);
	return [...keys];
}
