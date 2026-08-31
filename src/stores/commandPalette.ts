import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import type { TerminalMatch } from "../types";
import type { ContentMatch, DirEntry } from "../types/fs";
import { listenContentSearch, newContentSearchId, startContentSearch } from "../utils/contentSearch";
import { appLogger } from "./appLogger";
import { repositoriesStore } from "./repositories";
import { terminalsStore } from "./terminals";

const RECENT_ACTIONS_KEY = "tui-commander-recent-actions";
const MAX_RECENT = 10;
const SEARCH_DEBOUNCE_MS = 300;
const CONTENT_SEARCH_MIN_CHARS = 3;
const FILENAME_SEARCH_MIN_CHARS = 1;
const TERMINAL_SEARCH_MIN_CHARS = 3;
const MAX_CONTENT_RESULTS = 200;
const MAX_TERMINAL_RESULTS = 200;

export type PaletteMode = "command" | "filename" | "content" | "terminal";

/** The six scope chips shown in the palette. "all"/"actions"/"prompts" filter
 *  the action category within command mode; "files"/"content"/"terminals"
 *  mirror the existing `!`/`?`/`~` prefix-derived search modes — selecting one
 *  of those three rewrites the query's prefix rather than adding a second,
 *  independent axis of filtering. */
export type PaletteScope = "all" | "actions" | "prompts" | "files" | "content" | "terminals";

const SCOPE_ORDER: PaletteScope[] = ["all", "actions", "prompts", "files", "content", "terminals"];

/** Query prefix for each search scope — the single existing source of truth
 *  (`mode()` below) for filename/content/terminal search already keys off
 *  these same characters. */
const SEARCH_SCOPE_PREFIX: Record<"files" | "content" | "terminals", string> = {
	files: "!",
	content: "?",
	terminals: "~",
};

function isSearchScope(scope: PaletteScope): scope is "files" | "content" | "terminals" {
	return scope === "files" || scope === "content" || scope === "terminals";
}

interface CommandPaletteState {
	isOpen: boolean;
	query: string;
	/** Action-category scope within command mode ("all"/"actions"/"prompts").
	 *  Meaningless while a search prefix is active — `scope()` below reports
	 *  "files"/"content"/"terminals" instead in that case, derived from the
	 *  query prefix the same way `mode()` already is. */
	actionFilter: "all" | "actions" | "prompts";
	recentActions: string[];
	/** Content search results (? prefix) */
	contentResults: ContentMatch[];
	contentSearching: boolean;
	contentError: string | null;
	/** When true, content search (? prefix) spans all indexed repos, not just the active one */
	contentAllRepos: boolean;
	/** Cross-repo search: repos whose index was still building, so they were not searched.
	 *  Non-zero means an empty result is "not searched yet", NOT a confirmed miss. */
	contentReposPending: number;
	/** Cross-repo search: repos actually searched. */
	contentReposSearched: number;
	/** Filename search results (! prefix) */
	filenameResults: DirEntry[];
	filenameSearching: boolean;
	/** Terminal buffer search results (~ prefix) */
	terminalResults: TerminalMatch[];
	terminalSearching: boolean;
}

function loadRecentActions(): string[] {
	try {
		const raw = localStorage.getItem(RECENT_ACTIONS_KEY);
		if (!raw) return [];
		const parsed = JSON.parse(raw);
		return Array.isArray(parsed) ? parsed.slice(0, MAX_RECENT) : [];
	} catch {
		return [];
	}
}

function createCommandPaletteStore() {
	const [state, setState] = createStore<CommandPaletteState>({
		isOpen: false,
		query: "",
		actionFilter: "all",
		recentActions: loadRecentActions(),
		contentResults: [],
		contentSearching: false,
		contentError: null,
		contentAllRepos: false,
		contentReposPending: 0,
		contentReposSearched: 0,
		filenameResults: [],
		filenameSearching: false,
		terminalResults: [],
		terminalSearching: false,
	});

	// Search lifecycle state
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;
	let cancelled = false;
	let unlistenBatch: (() => void) | null = null;

	function cleanupSearch(): void {
		cancelled = true;
		if (debounceTimer) {
			clearTimeout(debounceTimer);
			debounceTimer = null;
		}
		unlistenBatch?.();
		unlistenBatch = null;
		setState({
			contentResults: [],
			contentSearching: false,
			contentError: null,
			filenameResults: [],
			filenameSearching: false,
			terminalResults: [],
			terminalSearching: false,
		});
	}

	/** Fire a filename search (non-streaming, single invoke) */
	function triggerFilenameSearch(searchQuery: string): void {
		const repoPath = repositoriesStore.state.activeRepoPath;
		if (!repoPath || searchQuery.length < FILENAME_SEARCH_MIN_CHARS) return;

		cancelled = false;
		setState({ filenameResults: [], filenameSearching: true });

		invoke<DirEntry[]>("search_files", { repoPath, query: searchQuery, limit: 50 })
			.then((results) => {
				if (!cancelled) {
					setState({ filenameResults: results, filenameSearching: false });
				}
			})
			.catch((err) => {
				if (!cancelled) {
					appLogger.error("app", "Filename search failed", err);
					setState({ filenameSearching: false });
				}
			});
	}

	/** Fire a content search with streaming results */
	async function triggerContentSearch(searchQuery: string): Promise<void> {
		const allRepos = state.contentAllRepos;
		const repoPath = repositoriesStore.state.activeRepoPath;
		if (searchQuery.length < CONTENT_SEARCH_MIN_CHARS) return;
		// Single-repo search needs an active repo; all-repos search does not.
		if (!allRepos && !repoPath) return;

		cancelled = false;
		setState({
			contentResults: [],
			contentSearching: true,
			contentError: null,
			contentReposPending: 0,
			contentReposSearched: 0,
		});

		// Subscribe to streaming results BEFORE invoking search
		const searchId = newContentSearchId();
		try {
			unlistenBatch = await listenContentSearch(searchId, {
				onBatch: (batch) => {
					if (cancelled) return;
					setState("contentResults", (prev) => {
						if (prev.length >= MAX_CONTENT_RESULTS) return prev;
						const combined = [...prev, ...batch.matches];
						return combined.slice(0, MAX_CONTENT_RESULTS);
					});
					setState({
						contentReposPending: batch.repos_pending ?? 0,
						contentReposSearched: batch.repos_searched ?? 0,
					});
					if (batch.is_final) {
						setState("contentSearching", false);
					}
				},
				onError: (message) => {
					if (cancelled) return;
					setState({ contentError: message, contentSearching: false });
				},
			});
		} catch (err) {
			appLogger.error("app", "Failed to subscribe to content search events", err);
			setState({ contentError: "Search setup failed", contentSearching: false });
			return;
		}

		if (cancelled) return;

		const invocation = allRepos
			? startContentSearch("search_content_all", { query: searchQuery, caseSensitive: false }, searchId)
			: startContentSearch(
					"search_content",
					{
						repoPath,
						query: searchQuery,
						caseSensitive: false,
						useRegex: false,
						wholeWord: false,
					},
					searchId,
				);
		invocation.catch((err) => {
			if (!cancelled) {
				appLogger.error("app", "Content search failed", err);
				setState({ contentError: String(err), contentSearching: false });
			}
		});
	}

	/** Search across all attached terminal buffers */
	async function triggerTerminalSearch(searchQuery: string): Promise<void> {
		cancelled = false;
		setState({ terminalResults: [], terminalSearching: true });

		const terminals = terminalsStore.state.terminals as Record<
			string,
			{ id: string; ref?: { searchBuffer?: (q: string) => TerminalMatch[] | Promise<TerminalMatch[]> } }
		>;
		const detached = terminalsStore.state.detachedWindows as Record<string, string>;

		// Fire every attached terminal's searchBuffer concurrently; the cancelled
		// check happens once at aggregation instead of per-round-trip.
		const searchers: Array<(q: string) => TerminalMatch[] | Promise<TerminalMatch[]>> = [];
		for (const id of Object.keys(terminals)) {
			// Skip detached terminals
			if (detached[id]) continue;
			const ref = terminals[id]?.ref;
			if (!ref?.searchBuffer) continue;
			searchers.push(ref.searchBuffer);
		}
		const settled = await Promise.allSettled(searchers.map((search) => search(searchQuery)));

		if (cancelled) return;

		const allResults: TerminalMatch[] = [];
		for (const result of settled) {
			if (result.status === "rejected") {
				appLogger.error("app", "Terminal buffer search failed", result.reason);
				continue;
			}
			allResults.push(...result.value);
			if (allResults.length >= MAX_TERMINAL_RESULTS) break;
		}

		setState({
			terminalResults: allResults.slice(0, MAX_TERMINAL_RESULTS),
			terminalSearching: false,
		});
	}

	/** Derived mode based on query prefix: ! = filename, ? = content, ~ = terminal */
	function mode(): PaletteMode {
		if (state.query.startsWith("!")) return "filename";
		if (state.query.startsWith("?")) return "content";
		if (state.query.startsWith("~")) return "terminal";
		return "command";
	}

	/** The effective search query (strips prefix character and leading space) */
	function searchQuery(): string {
		if (state.query.startsWith("!") || state.query.startsWith("?") || state.query.startsWith("~"))
			return state.query.slice(1).trimStart();
		return "";
	}

	/** The active scope chip. In command mode this is the action-category filter;
	 *  in a search mode it mirrors the query's prefix, so typing `!`/`?`/`~`
	 *  directly (without touching a chip) still lights up the matching chip. */
	function scope(): PaletteScope {
		const currentMode = mode();
		if (currentMode === "filename") return "files";
		if (currentMode === "content") return "content";
		if (currentMode === "terminal") return "terminals";
		return state.actionFilter;
	}

	/** The text the user has typed, independent of scope: the plain query in
	 *  command mode, or the prefix-stripped query in a search mode. Used to
	 *  carry typed text across a scope switch instead of discarding it. */
	function typedText(): string {
		return mode() === "command" ? state.query : searchQuery();
	}

	/** Switch scope chips. Moving to a search scope rewrites the query's prefix
	 *  (reusing the existing `!`/`?`/`~` mode machinery via `setQuery`); moving
	 *  to an action-category scope strips any prefix and updates the category
	 *  filter instead. Either way, whatever text was already typed carries over. */
	function setScope(next: PaletteScope): void {
		const text = typedText();
		if (isSearchScope(next)) {
			setQuery(text ? `${SEARCH_SCOPE_PREFIX[next]} ${text}` : `${SEARCH_SCOPE_PREFIX[next]} `);
			return;
		}
		setState("actionFilter", next);
		if (mode() !== "command") setQuery(text);
	}

	/** Move to the next/previous scope chip, wrapping at either end. */
	function cycleScope(direction: 1 | -1): void {
		const currentIndex = SCOPE_ORDER.indexOf(scope());
		const nextIndex = (currentIndex + direction + SCOPE_ORDER.length) % SCOPE_ORDER.length;
		setScope(SCOPE_ORDER[nextIndex]);
	}

	function open(): void {
		cleanupSearch();
		setState({ isOpen: true, query: "", actionFilter: "all" });
	}

	function close(): void {
		cleanupSearch();
		setState({ isOpen: false, query: "", actionFilter: "all" });
	}

	function toggle(): void {
		if (state.isOpen) {
			close();
		} else {
			open();
		}
	}

	function setQuery(query: string): void {
		const prevMode = mode();
		setState("query", query);
		const newMode = mode();

		// Mode changed → cleanup previous search
		if (prevMode !== newMode && prevMode !== "command") {
			cleanupSearch();
		}

		if (newMode === "filename") {
			const nextQuery = query.slice(1).trimStart();
			if (debounceTimer) {
				clearTimeout(debounceTimer);
				debounceTimer = null;
			}
			cancelled = true;

			if (nextQuery.length >= FILENAME_SEARCH_MIN_CHARS) {
				debounceTimer = setTimeout(() => triggerFilenameSearch(nextQuery), SEARCH_DEBOUNCE_MS);
			} else {
				setState({ filenameResults: [], filenameSearching: false });
			}
		} else if (newMode === "content") {
			const nextQuery = query.slice(1).trimStart();
			if (debounceTimer) {
				clearTimeout(debounceTimer);
				debounceTimer = null;
			}
			cancelled = true;
			unlistenBatch?.();
			unlistenBatch = null;

			if (nextQuery.length >= CONTENT_SEARCH_MIN_CHARS) {
				setState("contentSearching", false);
				debounceTimer = setTimeout(() => triggerContentSearch(nextQuery), SEARCH_DEBOUNCE_MS);
			} else {
				setState({ contentResults: [], contentSearching: false, contentError: null });
			}
		} else if (newMode === "terminal") {
			const nextQuery = query.slice(1).trimStart();
			if (debounceTimer) {
				clearTimeout(debounceTimer);
				debounceTimer = null;
			}
			cancelled = true;

			if (nextQuery.length >= TERMINAL_SEARCH_MIN_CHARS) {
				debounceTimer = setTimeout(() => triggerTerminalSearch(nextQuery), SEARCH_DEBOUNCE_MS);
			} else {
				setState({ terminalResults: [], terminalSearching: false });
			}
		}
	}

	/** Toggle content search between the active repo and all indexed repos,
	 *  re-running the current query immediately when in content mode. */
	function setContentAllRepos(value: boolean): void {
		if (state.contentAllRepos === value) return;
		setState("contentAllRepos", value);
		if (mode() !== "content") return;
		const nextQuery = searchQuery();
		cancelled = true;
		unlistenBatch?.();
		unlistenBatch = null;
		if (nextQuery.length >= CONTENT_SEARCH_MIN_CHARS) {
			setState({ contentResults: [], contentError: null });
			triggerContentSearch(nextQuery);
		} else {
			setState({ contentResults: [], contentSearching: false, contentError: null });
		}
	}

	/** Open palette with a pre-filled query (e.g. "~ " for terminal search mode) */
	function openWithQuery(query: string): void {
		cleanupSearch();
		setState({ isOpen: true, query, actionFilter: "all" });
		// Re-run setQuery to trigger mode-specific search logic
		setQuery(query);
	}

	function recordUsage(actionId: string): void {
		const updated = [actionId, ...state.recentActions.filter((id) => id !== actionId)].slice(0, MAX_RECENT);
		setState("recentActions", updated);
		try {
			localStorage.setItem(RECENT_ACTIONS_KEY, JSON.stringify(updated));
		} catch (err) {
			appLogger.warn("app", "Failed to persist recent actions to localStorage", err);
		}
	}

	// Methods are plain closures, never `this`-bound: every one of them is handed
	// around as a bare reference (keyboard handler map, action registry), which
	// would strip a `this` binding.
	return {
		state,
		mode,
		searchQuery,
		scope,
		setScope,
		cycleScope,
		open,
		close,
		toggle,
		setQuery,
		setContentAllRepos,
		openWithQuery,
		recordUsage,
	};
}

export const commandPaletteStore = createCommandPaletteStore();
