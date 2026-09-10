import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
import type { RepoChoice } from "./hooks/useRepoPickerDialog";
import { invoke } from "./invoke";
import { appLogger } from "./stores/appLogger";
import { pluginStore } from "./stores/pluginStore";
import { placementBranchFor, repositoriesStore } from "./stores/repositories";
import { resolvePlacementForCwd } from "./stores/terminalPlacement";
import { isTauri } from "./transport";

/** Bound on how many panes one `tuic://open-terminal` invocation can open.
 *  The `tuic open-here` CLI already caps at the same number before firing the
 *  deep link, but this URL can be fired directly (bypassing the CLI), so the
 *  cap is enforced here too — defense in depth against a crafted link fanning
 *  out an unbounded number of PTY spawns. */
const MAX_OPEN_TERMINAL_PATHS = 5;

/** Callbacks provided by App.tsx to control UI navigation */
export interface DeepLinkCallbacks {
	openSettings: (tab?: string) => void;
	/** Show an in-app confirmation dialog — replaces native browser confirm() */
	confirm: (title: string, message: string) => Promise<boolean>;
	/** Show an in-app error notification — replaces native browser alert() */
	onInstallError: (message: string) => void;
	/** Add a not-yet-known repo by path and make it active (same flow as the
	 *  sidebar's "Add Repository"). Used by `tuic <dir>`. */
	openRepoPath: (path: string) => Promise<void>;
	/** Ask which repo a Finder-invoked path (`tuic://open-terminal`) that
	 *  matched no repo and no active repo should open under. `null` = cancel. */
	chooseRepoForPath: (path: string) => Promise<RepoChoice | null>;
	/** Create+attach a terminal at `repoPath`/`branchName`, with `cwd`
	 *  overriding the branch's own worktree path. Mirrors
	 *  `createBranchSelectionCoordinator`'s `handleAddTerminalToBranch`. */
	handleAddTerminalToBranch: (repoPath: string, branchName: string, cwd?: string) => Promise<string | undefined>;
	/** Open a plain terminal at `cwd` with no repo/branch association. */
	openUnattachedTerminal: (cwd: string) => Promise<void>;
	/** Clear a terminal's recorded owner (`repoPath: null`) after it has
	 *  already been filed under a branch for display. Used only for the
	 *  `isGuess` placement rung — see `resolvePlacementForCwd`'s doc comment:
	 *  a guessed placement must never be recorded as real ownership, or
	 *  `reclaimParkedTerminal` stops reconsidering it on a later `cd`. */
	markTerminalPlacementAsGuess: (terminalId: string) => void;
}

/**
 * Where a single Finder-invoked path should open, and what to do about it.
 * `resolvePlacementForCwd` already covers the first two rungs of the ladder
 * (owning repo, then active repo); this only runs when that returns `null`,
 * i.e. the third rung — ask the user.
 */
async function openTerminalAtPath(path: string, callbacks: DeepLinkCallbacks): Promise<void> {
	const placement = resolvePlacementForCwd(path);
	if (placement) {
		const id = await callbacks.handleAddTerminalToBranch(placement.repoPath, placement.branchName, path);
		// isGuess means nothing actually claims this cwd — the active repo only
		// lent it a slot to render in. Filing it under that branch is still
		// correct (it needs to be visible somewhere), but recording repoPath as
		// that guessed repo would be a lie: it would stop reclaimParkedTerminal
		// from reconsidering this tab the next time its owner can be resolved.
		if (placement.isGuess && id) {
			callbacks.markTerminalPlacementAsGuess(id);
		}
		return;
	}

	const choice = await callbacks.chooseRepoForPath(path);
	if (!choice) return;

	switch (choice.kind) {
		case "repo": {
			// The user picked an existing repo, not a specific branch — resolve one
			// the same way a root-checkout match would (activeBranch, then whichever
			// branch records the repo root as its worktree). A registered repo with
			// no resolvable branch at all is a defensive edge case, not an expected
			// one: every repo-registration path seeds at least one branch.
			const branchName = placementBranchFor({ repoPath: choice.repoPath, branchName: null });
			if (branchName) {
				await callbacks.handleAddTerminalToBranch(choice.repoPath, branchName, path);
			} else {
				await callbacks.openUnattachedTerminal(path);
			}
			break;
		}
		case "register":
			// Registers the exact clicked path as a new repo root, which also
			// auto-spawns its first terminal there (`addRepoByPath`) — no separate
			// handleAddTerminalToBranch call needed.
			await callbacks.openRepoPath(path);
			break;
		case "unattached":
			await callbacks.openUnattachedTerminal(path);
			break;
	}
}

/** Parse a tuic:// URL into a command, path segments, and parameters */
function parseDeepLink(urlString: string): { command: string; pathSegments: string[]; params: URLSearchParams } | null {
	try {
		const url = new URL(urlString);
		if (url.protocol !== "tuic:") return null;
		// URL hostname is the command (tuic://install-plugin?url=... or tuic://cmd/ui/toast?title=Hello)
		const command = url.hostname;
		// pathname segments after the leading slash, e.g. "/ui/toast" → ["ui", "toast"]
		const pathSegments = url.pathname.split("/").filter(Boolean);
		return { command, pathSegments, params: url.searchParams };
	} catch {
		return null;
	}
}

/** Read-only / notify cmd/ actions that run WITHOUT confirmation. Everything
 *  else under cmd/ (destructive OR unknown) requires user confirmation — a
 *  malicious page can open a tuic:// URL unattended, so we default-deny rather
 *  than enumerate destructive actions (which drifts as new tools land, e.g.
 *  agent/send and session/pause were previously un-gated). Mirror of the
 *  loopback guards in mcp_transport.rs. */
const SAFE_COMMANDS = new Set([
	"ui/toast",
	"repo/list",
	"repo/active",
	"repo/prs",
	"repo/status",
	"repo/worktree_list",
	"session/list",
	"session/output",
	"session/status",
	"agent/detect",
	"agent/stats",
	"agent/list_peers",
	"agent/inbox",
]);

/** Commands that are blocked entirely via deep link, even with confirmation. */
const BLOCKED_COMMANDS = new Set(["config/save", "debug/invoke_js"]);

/** Handle a single deep link URL. Exported for tests. */
export async function handleDeepLink(urlString: string, callbacks: DeepLinkCallbacks): Promise<void> {
	const parsed = parseDeepLink(urlString);
	if (!parsed) {
		appLogger.warn("app", `Unrecognised deep link URL: ${urlString}`);
		return;
	}

	const { command, pathSegments, params } = parsed;

	switch (command) {
		case "install-plugin": {
			const url = params.get("url");
			if (!url) {
				appLogger.warn("app", "Deep link install-plugin: missing url parameter");
				return;
			}
			// Security: HTTPS only
			if (!url.startsWith("https://")) {
				appLogger.warn("app", "Deep link install-plugin: only HTTPS URLs are allowed");
				return;
			}
			// Confirmation dialog before downloading
			const proceed = await callbacks.confirm(
				"Install plugin?",
				`Install plugin from:\n${url}\n\nThis will download and install a plugin.`,
			);
			if (!proceed) return;

			try {
				await pluginStore.installFromUrl(url);
				// Open plugins tab so user can see the result
				callbacks.openSettings("plugins");
			} catch (err) {
				appLogger.error("plugin", "DeepLink: install-plugin failed", err);
				callbacks.onInstallError(`Plugin installation failed: ${err}`);
			}
			break;
		}

		case "open-repo": {
			const path = params.get("path");
			if (!path) {
				appLogger.warn("app", "Deep link open-repo: missing path parameter");
				return;
			}
			// Known repo → activate, no questions asked (this is `tuic .` in a repo
			// you already work in). Unknown repo → confirm first: a deep link can be
			// opened by any local page, and adding a repo starts a watcher and an
			// index over that directory.
			if (path in repositoriesStore.state.repositories) {
				repositoriesStore.setActive(path);
				break;
			}
			const proceed = await callbacks.confirm(
				"Add repository?",
				`Add and open this folder in TUICommander?\n\n${path}`,
			);
			if (!proceed) return;
			await callbacks.openRepoPath(path);
			break;
		}

		case "open-terminal": {
			const rawPaths = params.getAll("path");
			if (rawPaths.length === 0) {
				appLogger.warn("app", "Deep link open-terminal: missing path parameter");
				return;
			}
			const paths = rawPaths.slice(0, MAX_OPEN_TERMINAL_PATHS);
			if (rawPaths.length > paths.length) {
				appLogger.warn(
					"app",
					`Deep link open-terminal: ${rawPaths.length} paths given, opening only the first ${MAX_OPEN_TERMINAL_PATHS}`,
				);
			}
			for (const path of paths) {
				await openTerminalAtPath(path, callbacks);
			}
			break;
		}

		case "settings": {
			const tab = params.get("tab") ?? undefined;
			callbacks.openSettings(tab);
			break;
		}

		case "oauth-callback": {
			// OAuth 2.1 authorization code response from the upstream MCP server's
			// authorization server. Extract code + state and hand them to the
			// backend, which exchanges the code, persists the tokens, and resumes
			// the upstream connection.
			const code = params.get("code");
			const oauthState = params.get("state");
			const authError = params.get("error");

			if (authError) {
				const description = params.get("error_description") ?? "";
				appLogger.error("app", `OAuth callback returned error: ${authError}${description ? ` (${description})` : ""}`);
				callbacks.onInstallError(`OAuth authorization failed: ${authError}${description ? ` — ${description}` : ""}`);
				return;
			}

			if (!code || !oauthState) {
				appLogger.warn("app", "Deep link oauth-callback: missing code or state parameter");
				return;
			}

			try {
				await invoke("mcp_oauth_callback", { code, oauthState });
				appLogger.info("app", "OAuth callback completed");
			} catch (err) {
				appLogger.error("app", "OAuth callback invoke failed", err);
				callbacks.onInstallError(`OAuth callback failed: ${err}`);
			}
			break;
		}

		case "cmd": {
			// Gateway: tuic://cmd/{tool}/{action}?{params}
			if (pathSegments.length < 2) {
				appLogger.warn("app", `Deep link cmd: requires tuic://cmd/{tool}/{action}, got: ${urlString}`);
				return;
			}
			const [tool, action] = pathSegments;
			const cmdKey = `${tool}/${action}`;

			// Blocked commands — never execute
			if (BLOCKED_COMMANDS.has(cmdKey)) {
				appLogger.warn("app", `Deep link cmd: blocked command: ${cmdKey}`);
				return;
			}

			// Default-deny: only explicit read-only/notify commands run silently.
			// Destructive AND unknown actions require confirmation so a page that
			// opens a tuic:// URL cannot act unattended.
			if (!SAFE_COMMANDS.has(cmdKey)) {
				const proceed = await callbacks.confirm(
					"Execute command?",
					`Allow deep link to run:\n${tool} → ${action}\n\nThis may modify sessions, send messages, or spawn processes.`,
				);
				if (!proceed) return;
			}

			// Convert URLSearchParams to a plain object for the Tauri command
			const cmdParams: Record<string, string> = {};
			params.forEach((value, key) => {
				cmdParams[key] = value;
			});

			try {
				const result = await invoke("deep_link_mcp_call", {
					tool,
					action,
					params: cmdParams,
				});
				appLogger.info("app", `Deep link cmd ${cmdKey} result`, result);
			} catch (err) {
				appLogger.error("app", `Deep link cmd ${cmdKey} failed`, err);
			}
			break;
		}

		default:
			appLogger.warn("app", `Deep link unknown command: ${command}`);
	}
}

/** Initialise the deep link listener. Call once from App.tsx onMount. */
export function initDeepLinkHandler(callbacks: DeepLinkCallbacks): void {
	if (!isTauri()) return;

	onOpenUrl((urls: string[]) => {
		for (const url of urls) {
			handleDeepLink(url, callbacks).catch((err) => appLogger.error("app", "Deep link handler error", err));
		}
	}).catch((err) => {
		appLogger.error("app", "DeepLink: Failed to register handler", err);
	});
}
