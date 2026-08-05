import { emitLocalEvent, invoke } from "../invoke";
import { isTauri } from "../transport";
import type { ContentSearchResult } from "../types/fs";

/** Content search commands that stream on desktop but answer inline over HTTP */
type ContentSearchCommand = "search_content" | "search_content_all";

/**
 * Start a content search, whatever the transport.
 *
 * The desktop command returns immediately and streams matches back as
 * `content-search-batch` events. The HTTP route computes the same result but
 * returns it in the response body — nothing is pushed — so a browser client
 * subscribed to the event would spin forever. Republish the body as one final
 * batch so every caller can keep listening to a single event.
 *
 * The result deliberately does NOT go through the `/events` SSE bus: that bus is
 * global, so one client's search hits would land in every other client's panel.
 */
export async function startContentSearch(command: ContentSearchCommand, args: Record<string, unknown>): Promise<void> {
	const result = await invoke<ContentSearchResult | null>(command, args);
	if (isTauri()) return;
	emitLocalEvent("content-search-batch", {
		matches: result?.matches ?? [],
		is_final: true,
		files_searched: result?.files_searched ?? 0,
		files_skipped: result?.files_skipped ?? 0,
		truncated: result?.truncated ?? false,
		repos_pending: result?.repos_pending ?? 0,
		repos_searched: result?.repos_searched ?? 0,
	});
}
