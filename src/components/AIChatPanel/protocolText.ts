/** Interpret TUIC markers in ACP answer text after streamed chunks are joined.
 *
 * The terminal's Rust parser reads VT rows and their wrap state. ACP supplies
 * markdown text in the browser as well as Tauri, so this projection handles
 * logical lines and leaves fenced examples alone. See docs/backend/output-parser.md.
 */
export interface ChatProtocolText {
	body: string;
	intent: { text: string; title: string | null } | null;
	suggestions: string[];
}

const ACK = /^TUICommander[\t ]+v[0-9][^\s]*[\t ]+is[\t ]+connected\.[\t ]*/;
const BULLET = "(?:[●⏺•◦][\\t ]+)?";
const INTENT = new RegExp(`^[\\t ]*${BULLET}intent:[\\t ]+(.+)$`);
const SUGGEST = new RegExp(`^[\\t ]*${BULLET}suggest:[\\t ]*\\[([^\\[\\]\\r\\n]*)\\][\\t ]*$`);
const TITLE = /^(.*?)\(([^)]+)\)\s*$/;

export function projectChatProtocolText(text: string): ChatProtocolText {
	const body: string[] = [];
	let intent: ChatProtocolText["intent"] = null;
	let suggestions: string[] = [];
	let fence: "`" | "~" | null = null;
	for (const [index, original] of text.split(/\r?\n/).entries()) {
		const fenceMatch = /^[ \t]{0,3}(`{3,}|~{3,})/.exec(original);
		if (fenceMatch) {
			const marker = fenceMatch[1][0] as "`" | "~";
			if (!fence || fence === marker) fence = fence ? null : marker;
			body.push(original);
			continue;
		}
		if (fence) {
			body.push(original);
			continue;
		}
		if (/^(?: {4}|\t)/.test(original)) {
			body.push(original);
			continue;
		}
		// The acknowledgement can precede the first intent on the same line.
		const line = index === 0 ? original.replace(ACK, "") : original;
		const intentMatch = INTENT.exec(line);
		if (intentMatch) {
			const raw = intentMatch[1].trim();
			const titleMatch = TITLE.exec(raw);
			const description = (titleMatch?.[1] ?? raw).trim();
			if (description.length >= 3 && description !== "...") {
				intent = { text: description, title: titleMatch?.[2].trim() || null };
				continue;
			}
		}
		const suggestMatch = SUGGEST.exec(line);
		if (suggestMatch) {
			const items = suggestMatch[1]
				.split("|")
				.map((item) => item.trim())
				.filter(Boolean);
			if (items.length >= 2 && items.length <= 4) {
				suggestions = items;
				continue;
			}
		}
		if (line) body.push(line);
	}
	return { body: body.join("\n").replace(/^\n+|\n+$/g, ""), intent, suggestions };
}
