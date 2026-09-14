/**
 * Shared "comment on selected diff lines → send to the agent" helper, used by
 * both `DiffTab` and `SessionDiffTab`. Locks the exact wire format both
 * surfaces must emit — diverging formats would be a silent regression for
 * whichever surface didn't get the fix.
 */
import type { SelectedLineInfo } from "./diffPatch";

export interface DiffCommentTarget {
	/** File path shown in the comment header. */
	filePath: string;
	startLine: number;
	endLine: number;
	lines: SelectedLineInfo[];
}

/** Strip characters that would corrupt the single-line `[path:Lx-Ly]` header. */
function sanitizePath(path: string): string {
	return path.replace(/[\r\n\0]/g, "");
}

/** Strip characters that would corrupt a code line embedded in the message body. */
function sanitizeLine(content: string): string {
	return content.replace(/[\r\0]/g, "");
}

/**
 * PURE. Formats a diff comment as `[path:L10-L20]\n<code>\n— <text>` (or
 * `[path:L10]` for a single line). `code` lines keep their `+`/`-`/` ` prefix.
 */
export function formatDiffComment(target: DiffCommentTarget, text: string): string {
	const lineRange =
		target.startLine === target.endLine ? `L${target.startLine}` : `L${target.startLine}-L${target.endLine}`;
	const codeSnippet = target.lines.map((l) => `${l.type}${sanitizeLine(l.content)}`).join("\n");
	return `[${sanitizePath(target.filePath)}:${lineRange}]\n${codeSnippet}\n— ${text}`;
}

export type SendCommentResult = "ok" | "no-terminal" | "empty" | "error";

/**
 * Formats and sends a diff comment to the given session. `send` is injected
 * so callers/tests don't need the full `usePty` hook.
 */
export async function sendDiffComment(
	target: DiffCommentTarget,
	text: string,
	sessionId: string | undefined,
	agentType: string | null | undefined,
	send: (sessionId: string, message: string, agentType: string | null) => Promise<void>,
): Promise<SendCommentResult> {
	const trimmed = text.trim();
	if (!trimmed) return "empty";
	if (!sessionId) return "no-terminal";
	if (target.lines.length === 0) return "empty";

	const message = formatDiffComment(target, trimmed);
	try {
		await send(sessionId, message, agentType ?? null);
		return "ok";
	} catch {
		return "error";
	}
}
