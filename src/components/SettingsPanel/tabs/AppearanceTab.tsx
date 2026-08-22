import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../../i18n";
import type { RepoGroup } from "../../../stores/repositories";
import { repositoriesStore } from "../../../stores/repositories";
import { settingsStore } from "../../../stores/settings";
import { uiStore } from "../../../stores/ui";
import { getThemeNames } from "../../../themes";
import { UiLegend } from "../../HelpPanel/UiLegend";
import { ColorSwatchPicker } from "../../shared/ColorSwatchPicker";
import { DEFAULT_COLOR_PRESETS } from "../../shared/colorPresets";
import { SettingSelect, SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

/** Preset colors for groups and sidebar */
export { DEFAULT_COLOR_PRESETS as PRESET_COLORS } from "../../shared/colorPresets";

/** Single group row in the settings list */
const GroupSettingsItem: Component<{
	group: RepoGroup;
}> = (props) => {
	const [editing, setEditing] = createSignal(false);
	const [editName, setEditName] = createSignal(props.group.name);
	const [nameError, setNameError] = createSignal("");

	const commitRename = () => {
		const name = editName().trim();
		if (!name) {
			setNameError(t("groups.error.nameEmpty", "Name cannot be empty"));
			return;
		}
		const ok = repositoriesStore.renameGroup(props.group.id, name);
		if (!ok) {
			setNameError(t("groups.error.nameExists", "A group with this name already exists"));
			return;
		}
		setNameError("");
		setEditing(false);
	};

	const cancelEdit = () => {
		setEditName(props.group.name);
		setNameError("");
		setEditing(false);
	};

	return (
		<div class={s.groupItem}>
			<div class={s.groupRow}>
				<Show
					when={editing()}
					fallback={
						<span class={s.groupName} onDblClick={() => setEditing(true)}>
							{props.group.name}
						</span>
					}
				>
					<input
						class={s.groupNameInput}
						value={editName()}
						onInput={(e) => {
							setEditName(e.currentTarget.value);
							setNameError("");
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter") commitRename();
							if (e.key === "Escape") cancelEdit();
						}}
						autofocus
					/>
				</Show>
				<button
					class={s.groupDeleteBtn}
					onClick={() => repositoriesStore.deleteGroup(props.group.id)}
					title={t("groups.btn.deleteGroup", "Delete group")}
				>
					×
				</button>
			</div>
			<Show when={nameError()}>
				<div class={s.groupNameError}>{nameError()}</div>
			</Show>
			<ColorSwatchPicker
				color={props.group.color}
				presets={DEFAULT_COLOR_PRESETS}
				onChange={(c) => repositoriesStore.setGroupColor(props.group.id, c)}
			/>
		</div>
	);
};

const themeOptions = () => Object.entries(getThemeNames()).map(([value, label]) => ({ value, label }));

export const AppearanceTab: Component = () => {
	const groups = () =>
		repositoriesStore.state.groupOrder.map((id) => repositoriesStore.state.groups[id]).filter(Boolean);

	return (
		<div class={s.section}>
			<h3>{t("appearance.heading.theme", "Theme")}</h3>

			<SettingSelect
				label={t("appearance.label.terminalTheme", "Terminal Theme")}
				value={settingsStore.state.theme}
				onChange={(v) => settingsStore.setTheme(v)}
				options={themeOptions()}
				hint={t("appearance.hint.terminalTheme", "Color theme for terminal output and app chrome")}
			/>

			<h3>{t("appearance.heading.tabs", "Tabs")}</h3>

			<SettingSelect
				label={t("appearance.label.splitTabMode", "Split Tab Mode")}
				value={settingsStore.state.splitTabMode}
				onChange={(v) => {
					if (v === "separate" || v === "unified") settingsStore.setSplitTabMode(v);
				}}
				options={[
					{ value: "separate", label: t("appearance.splitTabMode.separate", "Separate") },
					{ value: "unified", label: t("appearance.splitTabMode.unified", "Unified") },
				]}
				hint={t("appearance.hint.splitTabMode", "How worktree tabs are arranged in the tab bar")}
			/>

			<SettingSelect
				label={t("appearance.label.tabOrderingMode", "Tab Ordering")}
				value={settingsStore.state.tabOrderingMode}
				onChange={(v) => {
					if (v === "grouped-by-type" || v === "terminals-first" || v === "free") settingsStore.setTabOrderingMode(v);
				}}
				options={[
					{ value: "grouped-by-type", label: t("appearance.tabOrderingMode.grouped", "Grouped by Type") },
					{ value: "terminals-first", label: t("appearance.tabOrderingMode.terminalsFirst", "Terminals First") },
					{ value: "free", label: t("appearance.tabOrderingMode.free", "Free") },
				]}
				hint={t(
					"appearance.hint.tabOrderingMode",
					"How tabs are ordered: grouped by type, terminals first, or freely interleaved",
				)}
			/>

			<SettingToggle
				checked={settingsStore.state.tabCyclingAllTypes}
				onChange={(v) => settingsStore.setTabCyclingAllTypes(v)}
				label={t("appearance.label.tabCyclingAllTypes", "Cycle All Tab Types")}
				hint={t(
					"appearance.hint.tabCyclingAllTypes",
					"Next/previous tab shortcuts cycle through diff, markdown and editor tabs too — not just terminals",
				)}
			/>

			<SettingToggle
				checked={settingsStore.state.tabTreeEnabled}
				onChange={(v) => settingsStore.setTabTreeEnabled(v)}
				label={t("appearance.label.tabTreeEnabled", "Nested Terminal Tabs")}
				hint={t(
					"appearance.hint.tabTreeEnabled",
					"Show a branch's open terminals as a collapsible list under its sidebar row — only when the branch has more than one terminal",
				)}
			/>

			<SettingSlider
				label={t("appearance.label.maxTabNameLength", "Max Tab Name Length")}
				value={settingsStore.state.maxTabNameLength}
				onChange={(v) => settingsStore.setMaxTabNameLength(v)}
				min={10}
				max={60}
				hint={t("appearance.hint.maxTabNameLength", "Maximum characters shown in tab names before truncating")}
			/>

			<h3>{t("appearance.heading.groups", "Repository Groups")}</h3>
			<p class={s.hint}>
				{t("appearance.hint.groups", "Organize repositories into color-coded groups in the sidebar")}
			</p>

			<Show when={groups().length === 0}>
				<div class={s.groupsEmpty}>{t("groups.empty.noGroups", "No groups yet")}</div>
			</Show>

			<For each={groups()}>{(group) => <GroupSettingsItem group={group} />}</For>

			<button
				class={s.groupsAddBtn}
				onClick={() => repositoriesStore.createGroup(t("groups.defaultGroupName", "New Group"))}
			>
				{t("groups.btn.addGroup", "Add Group")}
			</button>

			<h3>{t("appearance.heading.layout", "Layout")}</h3>

			<div class={s.group}>
				<button class={s.testBtn} onClick={() => uiStore.resetLayout()}>
					{t("appearance.btn.resetLayout", "Reset Panel Sizes")}
				</button>
				<p class={s.hint}>{t("appearance.hint.resetLayout", "Reset sidebar and panel widths to default values")}</p>
			</div>

			<h3>{t("appearance.heading.uiLegend", "UI Legend")}</h3>
			<p class={s.hint} style={{ "margin-bottom": "12px" }}>
				{t("appearance.hint.uiLegend", "Visual reference for colors, symbols, and badges used throughout the app")}
			</p>
			<UiLegend />
		</div>
	);
};
