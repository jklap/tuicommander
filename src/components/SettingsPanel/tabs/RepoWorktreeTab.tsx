import { type Component, For, Show } from "solid-js";
import { t } from "../../../i18n";
import { isMacOS } from "../../../platform";
import type {
	AutoDeleteOnPrClose,
	MergeStrategy,
	OrphanCleanup,
	RepoDefaults,
	WorktreeAfterMerge,
	WorktreeStorage,
} from "../../../stores/repoDefaults";
import type { RepoSettings } from "../../../stores/repoSettings";
import { settingsStore } from "../../../stores/settings";
import { ColorSwatchPicker } from "../../shared/ColorSwatchPicker";
import { DEFAULT_COLOR_PRESETS } from "../../shared/colorPresets";
import { TriStateToggle } from "../../shared/TriStateToggle";
import s from "../Settings.module.css";

export interface RepoTabProps {
	settings: RepoSettings;
	defaults: RepoDefaults;
	onUpdate: <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => void;
}

/** "inherit" sentinel value for nullable dropdowns */
const INHERIT = "__inherit__";

export const RepoWorktreeTab: Component<RepoTabProps> = (props) => {
	const branchOptions = [
		{ value: "automatic", label: t("repoWorktree.baseBranch.automatic", "Automatic") },
		{ value: "main", label: "main" },
		{ value: "master", label: "master" },
		{ value: "develop", label: "develop" },
	];

	const baseBranchValue = () => props.settings.baseBranch ?? INHERIT;

	const handleBaseBranchChange = (value: string) => {
		props.onUpdate("baseBranch", value === INHERIT ? null : value);
	};

	return (
		<div class={s.section}>
			<h3>{t("repoWorktree.heading.repository", "Repository")}</h3>

			<div class={s.group}>
				<label>{t("repoWorktree.label.displayName", "Display Name")}</label>
				<input
					type="text"
					value={props.settings.displayName ?? ""}
					onInput={(e) => props.onUpdate("displayName", e.currentTarget.value)}
					placeholder={t("repoWorktree.placeholder.displayName", "Custom name...")}
				/>
				<p class={s.hint}>{t("repoWorktree.hint.displayName", "Shown in sidebar instead of folder name")}</p>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.sidebarColor", "Sidebar Color")}</label>
				<ColorSwatchPicker
					color={props.settings.color ?? ""}
					presets={DEFAULT_COLOR_PRESETS}
					onChange={(c) => props.onUpdate("color", c)}
				/>
				<p class={s.hint}>{t("repoWorktree.hint.sidebarColor", "Color-code this repo in the sidebar")}</p>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.autoFetchInterval", "Auto-Fetch Interval")}</label>
				<select
					value={
						props.settings.autoFetchIntervalMinutes != null ? String(props.settings.autoFetchIntervalMinutes) : INHERIT
					}
					onChange={(e) => {
						const v = e.currentTarget.value;
						props.onUpdate("autoFetchIntervalMinutes", v === INHERIT ? null : Number(v));
					}}
				>
					<option value={INHERIT}>
						{t("repoWorktree.autoFetch.useDefault", "Use global default ({default})", {
							default:
								props.defaults.autoFetchIntervalMinutes === 0
									? t("repoWorktree.autoFetch.disabled", "Disabled")
									: `${props.defaults.autoFetchIntervalMinutes} min`,
						})}
					</option>
					<option value="0">{t("repoWorktree.autoFetch.disabled", "Disabled")}</option>
					<option value="5">5 min</option>
					<option value="15">15 min</option>
					<option value="30">30 min</option>
					<option value="60">60 min</option>
				</select>
				<p class={s.hint}>
					{t("repoWorktree.hint.autoFetch", "Periodically fetch from remote to keep branch stats fresh")}
				</p>
			</div>

			<h3>{t("repoWorktree.heading.worktreeConfiguration", "Worktree Configuration")}</h3>

			<div class={s.group}>
				<label>{t("repoWorktree.label.branchFrom", "Branch From")}</label>
				<select value={baseBranchValue()} onChange={(e) => handleBaseBranchChange(e.currentTarget.value)}>
					<option value={INHERIT}>
						{t("repoWorktree.baseBranch.useGlobalDefault", "Use global default ({default})", {
							default: props.defaults.baseBranch,
						})}
					</option>
					<For each={branchOptions}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
				</select>
				<p class={s.hint}>{t("repoWorktree.hint.branchFrom", "Base branch for new worktrees")}</p>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.fileHandling", "File Handling")}</label>

				<TriStateToggle
					value={props.settings.copyIgnoredFiles}
					onChange={(v) => props.onUpdate("copyIgnoredFiles", v)}
					label={t("repoWorktree.toggle.copyIgnoredFiles", "Copy ignored files")}
					inherited={props.defaults.copyIgnoredFiles}
				/>

				<TriStateToggle
					value={props.settings.copyUntrackedFiles}
					onChange={(v) => props.onUpdate("copyUntrackedFiles", v)}
					label={t("repoWorktree.toggle.copyUntrackedFiles", "Copy untracked files")}
					inherited={props.defaults.copyUntrackedFiles}
				/>
			</div>

			<h3>{t("repoWorktree.heading.worktreeSettings", "Worktree Settings")}</h3>

			<div class={s.group}>
				<label>{t("repoWorktree.label.worktreeStorage", "Storage Strategy")}</label>
				<select
					value={props.settings.worktreeStorage ?? INHERIT}
					onChange={(e) =>
						props.onUpdate(
							"worktreeStorage",
							e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as WorktreeStorage),
						)
					}
				>
					<option value={INHERIT}>
						{t("repoWorktree.worktreeStorage.useDefault", "Use global default ({default})", {
							default: props.defaults.worktreeStorage,
						})}
					</option>
					<option value="sibling">{t("repoWorktree.worktreeStorage.sibling", "Sibling directory (__wt)")}</option>
					<option value="app-dir">{t("repoWorktree.worktreeStorage.appDir", "App config directory")}</option>
					<option value="inside-repo">
						{t("repoWorktree.worktreeStorage.insideRepo", "Inside repository (.worktrees)")}
					</option>
					<option value="claude-code-default">
						{t("repoWorktree.worktreeStorage.claudeCodeDefault", "Claude Code default (.claude/worktrees)")}
					</option>
				</select>
			</div>

			<div class={s.group}>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={props.settings.autoConsolidateWorktrees}
						onChange={(e) => props.onUpdate("autoConsolidateWorktrees", e.currentTarget.checked)}
					/>
					<span>
						{t("repoWorktree.toggle.autoConsolidate", "Show all worktrees of this repo in one consolidated screen")}
					</span>
				</div>
			</div>

			<div class={s.group}>
				<TriStateToggle
					value={props.settings.promptOnCreate}
					onChange={(v) => props.onUpdate("promptOnCreate", v)}
					label={t("repoWorktree.toggle.promptOnCreate", "Prompt for branch name during creation")}
					inherited={props.defaults.promptOnCreate}
				/>
			</div>

			<div class={s.group}>
				<TriStateToggle
					value={props.settings.deleteBranchOnRemove}
					onChange={(v) => props.onUpdate("deleteBranchOnRemove", v)}
					label={t("repoWorktree.toggle.deleteBranchOnRemove", "Delete local branch when removing worktree")}
					inherited={props.defaults.deleteBranchOnRemove}
				/>
			</div>

			<div class={s.group}>
				<TriStateToggle
					value={props.settings.autoArchiveMerged}
					onChange={(v) => props.onUpdate("autoArchiveMerged", v)}
					label={t("repoWorktree.toggle.autoArchiveMerged", "Auto-archive merged worktrees")}
					inherited={props.defaults.autoArchiveMerged}
				/>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.orphanCleanup", "Orphan Worktree Cleanup")}</label>
				<select
					value={props.settings.orphanCleanup ?? INHERIT}
					onChange={(e) =>
						props.onUpdate(
							"orphanCleanup",
							e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as OrphanCleanup),
						)
					}
				>
					<option value={INHERIT}>
						{t("repoWorktree.orphanCleanup.useDefault", "Use global default ({default})", {
							default: props.defaults.orphanCleanup,
						})}
					</option>
					<option value="ask">{t("repoWorktree.orphanCleanup.ask", "Ask before archiving")}</option>
					<option value="on">{t("repoWorktree.orphanCleanup.on", "Auto-archive")}</option>
					<option value="delete">{t("repoWorktree.orphanCleanup.delete", "Auto-remove (delete, no archive)")}</option>
					<option value="off">{t("repoWorktree.orphanCleanup.off", "Keep (mark as detached)")}</option>
				</select>
				<p class={s.hint}>
					{t(
						"repoWorktree.hint.orphanCleanup",
						"A worktree whose branch was deleted out from under it. 'Auto-archive' and 'Ask' move it aside (recoverable) since detection is a heuristic that can misfire; 'Auto-remove' deletes it outright with no recovery.",
					)}
				</p>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.prMergeStrategy", "PR Merge Strategy")}</label>
				<select
					value={props.settings.prMergeStrategy ?? INHERIT}
					onChange={(e) =>
						props.onUpdate(
							"prMergeStrategy",
							e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as MergeStrategy),
						)
					}
				>
					<option value={INHERIT}>
						{t("repoWorktree.mergeStrategy.useDefault", "Use global default ({default})", {
							default: props.defaults.prMergeStrategy,
						})}
					</option>
					<option value="merge">{t("repoWorktree.mergeStrategy.merge", "Merge")}</option>
					<option value="squash">{t("repoWorktree.mergeStrategy.squash", "Squash")}</option>
					<option value="rebase">{t("repoWorktree.mergeStrategy.rebase", "Rebase")}</option>
				</select>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.afterMerge", "After Merge Behavior")}</label>
				<select
					value={props.settings.afterMerge ?? INHERIT}
					onChange={(e) =>
						props.onUpdate(
							"afterMerge",
							e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as WorktreeAfterMerge),
						)
					}
				>
					<option value={INHERIT}>
						{t("repoWorktree.afterMerge.useDefault", "Use global default ({default})", {
							default: props.defaults.afterMerge,
						})}
					</option>
					<option value="archive">{t("repoWorktree.afterMerge.archive", "Archive worktree")}</option>
					<option value="delete">{t("repoWorktree.afterMerge.delete", "Delete worktree")}</option>
					<option value="ask">{t("repoWorktree.afterMerge.ask", "Ask each time")}</option>
				</select>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.autoDeleteOnPrClose", "Auto-Delete on PR Close")}</label>
				<select
					value={props.settings.autoDeleteOnPrClose ?? INHERIT}
					onChange={(e) =>
						props.onUpdate(
							"autoDeleteOnPrClose",
							e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as AutoDeleteOnPrClose),
						)
					}
				>
					<option value={INHERIT}>
						{t("repoWorktree.autoDelete.useDefault", "Use global default ({default})", {
							default: props.defaults.autoDeleteOnPrClose,
						})}
					</option>
					<option value="off">{t("repoWorktree.autoDelete.off", "Off")}</option>
					<option value="ask">{t("repoWorktree.autoDelete.ask", "Ask before deleting")}</option>
					<option value="auto">{t("repoWorktree.autoDelete.auto", "Auto-delete silently")}</option>
				</select>
				<p class={s.hint}>{t("repoWorktree.hint.autoDelete", "Delete local branch when its PR is merged or closed")}</p>
			</div>

			<div class={s.group}>
				<label>{t("repoWorktree.label.prVisibility", "PR Visibility")}</label>
				<TriStateToggle
					value={props.settings.prHideDrafts}
					onChange={(v) => props.onUpdate("prHideDrafts", v)}
					label={t("repoWorktree.toggle.prHideDrafts", "Hide Draft PRs")}
					inherited={settingsStore.state.prHideDrafts}
				/>
				<TriStateToggle
					value={props.settings.prHideConflicting}
					onChange={(v) => props.onUpdate("prHideConflicting", v)}
					label={t("repoWorktree.toggle.prHideConflicting", "Hide Conflicting PRs")}
					inherited={settingsStore.state.prHideConflicting}
				/>
				<TriStateToggle
					value={props.settings.prHideCiFailing}
					onChange={(v) => props.onUpdate("prHideCiFailing", v)}
					label={t("repoWorktree.toggle.prHideCiFailing", "Hide CI Failing PRs")}
					inherited={settingsStore.state.prHideCiFailing}
				/>
			</div>

			<Show when={isMacOS()}>
				<div class={s.group}>
					<label>{t("repoWorktree.label.terminal", "Terminal")}</label>

					<TriStateToggle
						value={props.settings.terminalMetaHotkeys}
						onChange={(v) => props.onUpdate("terminalMetaHotkeys", v)}
						label={t("repoWorktree.toggle.terminalMetaHotkeys", "Enable Cmd+1-9 terminal hotkeys")}
						inherited={true}
					/>
				</div>
			</Show>
		</div>
	);
};
