import { type Component, createEffect, createMemo, createResource, createSignal, onCleanup, Show } from "solid-js";
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
import { SettingsSearchBox, SettingsSearchResults, scrollToSetting } from "./SettingsSearch";
import type { SettingsShellTab } from "./SettingsShell";
import { SettingsShell } from "./SettingsShell";
import {
	entryLabel,
	entrySection,
	type SettingsSearchEntry,
	type SettingsSearchTarget,
	searchSettings,
} from "./settingsSearchIndex";
import { getGlobalTabs } from "./settingsTabs";
import {
	AgentsTab,
	AiChatTab,
	AppearanceTab,
	GeneralTab,
	GitHubTab,
	NotificationsTab,
	PluginsTab,
	RemoteAccessTab,
	RepoScriptsTab,
	RepoWorktreeTab,
	SelectionTab,
	ServicesTab,
	SmartPromptsTab,
	StreamDockTab,
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
	/** Rendered section/label text to scroll to and flash once the panel is
	 * open — how a Command Palette "Settings" action lands on its control */
	initialTarget?: SettingsSearchTarget;
	context?: SettingsContext;
}

function defaultTab(ctx: SettingsContext): string {
	if (ctx.kind === "repo") return `repo:${ctx.repoPath}`;
	return "general";
}

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

	const [query, setQuery] = createSignal("");

	// Reset active tab when context changes or panel opens. Also re-runs when a
	// palette settings action fires while the panel is already open (the caller
	// swaps initialTab/initialTarget), so a second deep link still lands.
	createEffect(() => {
		if (props.visible) {
			setActiveTab(resolveInitialTab());
			// A stale query would hide the tab the caller asked for behind results
			setQuery("");
			if (props.initialTarget) setPendingTarget(props.initialTarget);
		}
	});

	/** Nav click handler: switches the pane and remembers it for next time. */
	const handleTabChange = (tab: string) => {
		setActiveTab(tab);
		uiStore.setLastSettingsTab(tab);
	};

	// Target a search result or a deep link asked for, consumed by the scroll
	// effect below
	const [pendingTarget, setPendingTarget] = createSignal<SettingsSearchTarget | null>(null);

	// Two callers need the panel scrolled to a block that sits below the fold:
	// a deep link (the MCP popup's "Manage in Settings", which names a DOM id)
	// and a search result (which names a section heading). Both have to wait for
	// the frame that inserts the tab content into the document.
	createEffect(() => {
		if (!props.visible) return;
		const target = pendingTarget();
		const section = props.initialSection;
		if (!target && !section) return;
		const frame = requestAnimationFrame(() => {
			if (target) {
				const content = document.querySelector("[data-settings-content]");
				if (content) scrollToSetting(content, target.section, target.label);
				setPendingTarget(null);
				return;
			}
			if (section) document.getElementById(section)?.scrollIntoView({ block: "start", behavior: "smooth" });
		});
		onCleanup(() => cancelAnimationFrame(frame));
	});

	/** Open the tab a search result lives in, then scroll to its section. */
	const openResult = (entry: SettingsSearchEntry) => {
		setQuery("");
		setActiveTab(entry.tab);
		setPendingTarget({ section: entrySection(entry), label: entryLabel(entry) });
	};

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

	// Only tabs the nav actually offers: Dictation is absent in browser mode and
	// AI Chat behind a flag, so their settings must not be offered either.
	const results = () => searchSettings(query(), new Set(getGlobalTabs().map((tab) => tab.key)));

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
			navHeader={<SettingsSearchBox value={query()} onInput={setQuery} />}
			footer={footer()}
		>
			<Show
				when={!query()}
				fallback={<SettingsSearchResults results={results()} tabs={buildNavItems()} onSelect={openResult} />}
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
				<Show when={activeTab() === "streamdock"}>
					<StreamDockTab />
				</Show>
				<Show when={activeTab() === "github"}>
					<GitHubTab />
				</Show>
				<Show when={activeTab() === "services"}>
					<ServicesTab />
				</Show>
				<Show when={activeTab() === "remote-access"}>
					<RemoteAccessTab />
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
