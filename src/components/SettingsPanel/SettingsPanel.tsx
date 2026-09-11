import { type Component, createEffect, createMemo, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import type { BaseRefOption } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { type RepoSettings, repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { isTauri } from "../../transport";
import { pathBasename } from "../../utils/pathUtils";
import { getRepoColor } from "../../utils/repoColor";
import { DictationSettings } from "./DictationSettings";
import s from "./Settings.module.css";
import type { SettingsShellTab } from "./SettingsShell";
import { SettingsShell } from "./SettingsShell";
import { type SettingsSearchResult, searchSettings } from "./settingsSearchIndex";
import {
	AgentsTab,
	AiChatTab,
	AppearanceTab,
	GeneralTab,
	GitHubTab,
	NotificationsTab,
	PluginsTab,
	RepoScriptsTab,
	RepoWorktreeTab,
	SelectionTab,
	ServicesTab,
	SmartPromptsTab,
	TerminalTab,
} from "./tabs";
import { ProvidersTab } from "./tabs/ProvidersTab";

/** Context for initial selection when opening the panel */
export type SettingsContext = { kind: "global" } | { kind: "repo"; repoPath: string; connectionId?: string };

export interface SettingsPanelProps {
	visible: boolean;
	onClose: () => void;
	initialTab?: string;
	/** DOM id of a block to scroll to once the panel is open — see sections.ts */
	initialSection?: string;
	context?: SettingsContext;
}

const BASE_GLOBAL_TABS: SettingsShellTab[] = [
	{ key: "general", label: t("settings.general", "General") },
	{ key: "appearance", label: t("settings.appearance", "Appearance") },
	{ key: "terminal", label: t("settings.terminal", "Terminal") },
	{ key: "selection", label: t("settings.selection", "Smart Selection") },
	{ key: "notifications", label: t("settings.notifications", "Notifications") },
	{ key: "dictation", label: t("settings.dictation", "Dictation") },
	{ key: "github", label: "Git & GitHub" },
	{ key: "services", label: t("settings.services", "Services & MCP") },
	{ key: "plugins", label: t("settings.plugins", "Plugins") },
	{ key: "smart-prompts", label: t("settings.smartPrompts", "Smart Prompts") },
	{ key: "providers", label: "Providers" },
	{ key: "agents", label: t("settings.agents", "Agents") },
];

function getGlobalTabs(): SettingsShellTab[] {
	const tabs = isTauri() ? BASE_GLOBAL_TABS : BASE_GLOBAL_TABS.filter((tab) => tab.key !== "dictation");
	if (settingsStore.isAiChatEnabled()) {
		return [...tabs, { key: "ai-chat", label: "AI Chat" }];
	}
	return tabs;
}

function defaultTab(ctx: SettingsContext): string {
	if (ctx.kind === "repo") return `repo:${ctx.repoPath}`;
	return "general";
}

const SearchResultsList: Component<{
	results: SettingsSearchResult[];
	onSelect: (result: SettingsSearchResult) => void;
}> = (props) => (
	<div class={s.searchResults}>
		<Show
			when={props.results.length > 0}
			fallback={<div class={s.searchEmpty}>{t("settings.search.empty", "No matching settings")}</div>}
		>
			<For each={props.results}>
				{(result) => (
					<button type="button" class={s.searchResultItem} onClick={() => props.onSelect(result)}>
						<div class={s.searchResultBreadcrumb}>
							{result.tabLabel} › {result.section}
						</div>
						<div class={s.searchResultLabel}>{result.label}</div>
						<Show when={result.hint}>
							<div class={s.searchResultHint}>{result.hint}</div>
						</Show>
					</button>
				)}
			</For>
		</Show>
	</div>
);

/** Build the full nav from global sections + configured repos */
function buildNavItems(): SettingsShellTab[] {
	// All repos, including those nested in groups — grouped repos live in
	// group.repoOrder, not state.repoOrder, so iterating repoOrder alone would
	// hide them from the Settings nav. (#64)
	const repos = repositoriesStore.getAllReposOrdered();

	const items: SettingsShellTab[] = [...getGlobalTabs()];

	if (repos.length > 0) {
		items.push({ key: "__sep__", label: "─" });
		items.push({ key: "__label__:Repositories", label: t("settings.repositories", "REPOSITORIES") });
		for (const repo of repos) {
			const label = repo.displayName || pathBasename(repo.path) || repo.path;
			const color = getRepoColor(repo.path);
			items.push({ key: `repo:${repo.path}`, label, color });
		}
	}

	return items;
}

export const SettingsPanel: Component<SettingsPanelProps> = (props) => {
	const ctx = () => props.context ?? { kind: "global" as const };

	// Memoized: buildNavItems() rebuilds the whole repo list (colors, display
	// names) and is read from both resolveInitialTab() and the JSX below —
	// without this it reran twice per reactive pass for no reason.
	const navItems = createMemo(buildNavItems);

	// Pane to open on: an explicit deep link wins, then an explicit repo context
	// (e.g. the git panel's "Repo Settings" action) jumps to that repo's tab,
	// otherwise fall back to the pane the user last selected this session —
	// provided it's still a real nav entry (a remembered repo tab whose repo
	// was removed, or "ai-chat"/"dictation" while unavailable, must not stick).
	const resolveInitialTab = (): string => {
		if (props.initialTab) return props.initialTab;
		if (ctx().kind === "repo") return defaultTab(ctx());
		const remembered = uiStore.state.lastSettingsTab;
		if (remembered && navItems().some((item) => item.key === remembered)) {
			return remembered;
		}
		return defaultTab(ctx());
	};

	const [activeTab, setActiveTab] = createSignal(resolveInitialTab());

	// Reset active tab when context changes or panel opens
	createEffect(() => {
		if (props.visible) {
			setActiveTab(resolveInitialTab());
		}
	});

	/** Nav click handler: switches the pane, remembers it for next time, and
	 *  leaves search mode (a direct nav click means "never mind the search"). */
	const handleTabChange = (tab: string) => {
		setActiveTab(tab);
		uiStore.setLastSettingsTab(tab);
		setSearchQuery("");
	};

	// Cross-pane settings search (the nav search box). Selecting a result
	// switches tab and queues a scroll+highlight for its control, once that
	// tab's content has actually mounted (see pendingJumpId effect below).
	const [searchQuery, setSearchQuery] = createSignal("");
	const searchResults = () => searchSettings(searchQuery());
	const [pendingJumpId, setPendingJumpId] = createSignal<string | null>(null);

	const selectSearchResult = (result: SettingsSearchResult) => {
		handleTabChange(result.tab);
		setPendingJumpId(result.controlId);
	};

	/** Scroll a control into view after the frame that mounts its tab's content,
	 *  optionally flashing the search-highlight animation on it. Shared by both
	 *  jump paths below — they differ only in scroll alignment and whether to
	 *  highlight, which had nearly drifted apart before being unified here. */
	const jumpToElement = (id: string | undefined, opts: { block: ScrollLogicalPosition; highlight?: boolean }) => {
		if (!id) return;
		const frame = requestAnimationFrame(() => {
			const el = document.getElementById(id);
			el?.scrollIntoView({ block: opts.block, behavior: "smooth" });
			if (opts.highlight && el) {
				el.classList.remove(s.searchHighlight);
				// Force a reflow so re-adding the class restarts the animation
				// even if the same control was just jumped to a moment ago.
				void el.offsetWidth;
				el.classList.add(s.searchHighlight);
			}
		});
		onCleanup(() => cancelAnimationFrame(frame));
	};

	// A deep link (the MCP popup's "Manage in Settings") opens a long tab where
	// the block it promised sits below the fold. Scroll to it, after the frame
	// that inserts the tab content into the document.
	createEffect(() => {
		if (!props.visible) return;
		jumpToElement(props.initialSection, { block: "start" });
	});

	// A settings-search selection made while the panel is already open — same
	// scroll mechanism as above, but re-triggerable (a prop only fires once
	// per value change; this is driven by our own signal instead) and it also
	// flashes the matched control so it's findable at a glance.
	createEffect(() => {
		const id = pendingJumpId();
		if (!id) return;
		jumpToElement(id, { block: "center", highlight: true });
		setPendingJumpId(null);
	});

	// Auto-reset to general when the current tab vanishes (e.g. AI Chat flag
	// toggled off while AI Chat tab is active). Without this, the body would
	// also disappear (per the Show guard above) but the nav would have no
	// highlight. (#1376-7333)
	createEffect(() => {
		if (activeTab() === "ai-chat" && !settingsStore.isAiChatEnabled()) {
			// handleTabChange, not a raw setActiveTab: also updates lastSettingsTab,
			// so a stale "ai-chat" can't resurface if the flag gets re-enabled
			// before Settings is reopened (#1376-7333 follow-up, 2026-09-10 review).
			handleTabChange("general");
		}
	});

	/** Repo path if a repo nav item is currently active, null otherwise */
	const activeRepoPath = (): string | null => {
		const tab = activeTab();
		return tab.startsWith("repo:") ? tab.slice(5) : null;
	};

	const activeConnectionId = (): string | undefined => {
		const path = activeRepoPath();
		return path ? repositoriesStore.getConnectionId(path) : undefined;
	};

	const repoSettings = (path: string) => repoSettingsStore.getOrCreate(path, shortenHomePath(path));

	// Real branch/ref list for the active repo's "Branch From" dropdown — refetched whenever
	// the active repo nav item changes. A failure (e.g. repo path no longer valid) just leaves
	// this undefined; RepoWorktreeTab must never treat "still loading" as "branch is missing".
	const [baseRefs] = createResource(activeRepoPath, async (path) => {
		try {
			return await invoke<BaseRefOption[]>("list_base_ref_options", { repoPath: path });
		} catch (err) {
			appLogger.warn("settings", "list_base_ref_options failed", { repoPath: path, error: String(err) });
			return undefined;
		}
	});

	const updateRepoSetting =
		(repoPath: string) =>
		<K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => {
			repoSettingsStore.update(repoPath, { [key]: value });
			if (key === "displayName") {
				repositoriesStore.setDisplayName(repoPath, value as string);
			}
		};

	/** Write this repo's UI settings into a committable `.tuic.json` at its root */
	const copyToProject = async (repoPath: string) => {
		try {
			await invoke("save_repo_local_config", { repoPath });
			toastsStore.add(
				t("settings.copyToProject.done", "Saved .tuic.json"),
				t("settings.copyToProject.doneHint", "Repo settings written to the project root — commit to share"),
				"info",
			);
		} catch (err) {
			toastsStore.add(t("settings.copyToProject.failed", "Failed to write .tuic.json"), String(err), "error");
		}
	};

	const footer = () => {
		const path = activeRepoPath();
		return (
			<div class={s.footer}>
				<Show when={path} fallback={<span />}>
					{(p) => (
						<button class={s.footerReset} onClick={() => repoSettingsStore.reset(p())}>
							{t("settings.resetToDefaults", "Reset to Defaults")}
						</button>
					)}
				</Show>
				<button class={s.footerDone} onClick={props.onClose}>
					{t("settings.done", "Done")}
				</button>
			</div>
		);
	};

	return (
		<SettingsShell
			visible={props.visible}
			onClose={props.onClose}
			title={t("settings.title", "Settings")}
			tabs={navItems()}
			activeTab={activeTab()}
			onTabChange={handleTabChange}
			navWidth={uiStore.state.settingsNavWidth}
			onNavWidthChange={uiStore.setSettingsNavWidth}
			onNavWidthPersist={uiStore.persistUIPrefs}
			searchQuery={searchQuery()}
			onSearchQueryChange={setSearchQuery}
			footer={footer()}
		>
			<Show
				when={!searchQuery().trim()}
				fallback={<SearchResultsList results={searchResults()} onSelect={selectSearchResult} />}
			>
				{/* Repo settings (shown when a repo nav item is active) */}
				<Show when={activeRepoPath()} keyed>
					{(path) => {
						const settings = repoSettings(path);
						const onUpdate = updateRepoSetting(path);
						return (
							<>
								<RepoWorktreeTab
									settings={settings}
									defaults={repoDefaultsStore.state}
									onUpdate={onUpdate}
									baseRefs={baseRefs()}
								/>
								<RepoScriptsTab settings={settings} defaults={repoDefaultsStore.state} onUpdate={onUpdate} />
								<Show when={isTauri()}>
									<div class={s.section}>
										<h3>{t("settings.copyToProject.heading", "Share with Team")}</h3>
										<p class={s.hint}>
											{t(
												"settings.copyToProject.hint",
												"Write this repo's worktree/branch settings to a .tuic.json in the project root. Commit it so teammates inherit the same defaults. Scripts are never exported.",
											)}
										</p>
										<div class={s.actions}>
											<button onClick={() => copyToProject(path)}>
												{t("settings.copyToProject.button", "Copy settings to .tuic.json")}
											</button>
										</div>
									</div>
								</Show>
							</>
						);
					}}
				</Show>

				{/* Global sections */}
				<Show when={activeTab() === "general"}>
					<GeneralTab />
				</Show>
				<Show when={activeTab() === "appearance"}>
					<AppearanceTab />
				</Show>
				<Show when={activeTab() === "terminal"}>
					<TerminalTab />
				</Show>
				<Show when={activeTab() === "selection"}>
					<SelectionTab />
				</Show>
				<Show when={activeTab() === "notifications"}>
					<NotificationsTab />
				</Show>
				<Show when={activeTab() === "dictation"}>
					<DictationSettings />
				</Show>
				<Show when={activeTab() === "github"}>
					<GitHubTab />
				</Show>
				<Show when={activeTab() === "services"}>
					<ServicesTab />
				</Show>
				<Show when={activeTab() === "plugins"}>
					<PluginsTab onClose={props.onClose} />
				</Show>
				<Show when={activeTab() === "smart-prompts"}>
					<SmartPromptsTab />
				</Show>
				<Show when={activeTab() === "providers"}>
					<ProvidersTab />
				</Show>
				<Show when={activeTab() === "agents"}>
					<AgentsTab connectionId={activeConnectionId()} />
				</Show>
				<Show when={activeTab() === "ai-chat" && settingsStore.isAiChatEnabled()}>
					<AiChatTab />
				</Show>
			</Show>
		</SettingsShell>
	);
};
