import type { Component } from "solid-js";
import { t } from "../../../i18n";
import { settingsStore } from "../../../stores/settings";
import { SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

export const TerminalTab: Component = () => {
	return (
		<div class={s.section}>
			<h3>{t("terminal.heading.blocks", "Blocks")}</h3>

			<SettingToggle
				checked={settingsStore.state.showBlockTimestamps}
				onChange={(v) => settingsStore.setShowBlockTimestamps(v)}
				label={t("terminal.toggle.showBlockTimestamps", "Show block timestamps")}
				hint={t(
					"terminal.hint.showBlockTimestamps",
					"Hold Ctrl+Cmd to reveal when each command block started, as relative time.",
				)}
			/>

			<SettingToggle
				checked={settingsStore.state.showBlockMarks}
				onChange={(v) => settingsStore.setShowBlockMarks(v)}
				label={t("terminal.toggle.showBlockMarks", "Show block marks")}
				hint={t(
					"terminal.hint.showBlockMarks",
					"Tick marks on the scrollbar for each command block — red when the command failed.",
				)}
			/>

			<SettingToggle
				checked={settingsStore.state.showPromptMarks}
				onChange={(v) => settingsStore.setShowPromptMarks(v)}
				label={t("terminal.toggle.showPromptMarks", "Show prompt marks")}
				hint={t("terminal.hint.showPromptMarks", "A green tick mark on the scrollbar for each prompt you sent.")}
			/>

			<SettingToggle
				checked={settingsStore.state.blockFoldingEnabled}
				onChange={(v) => settingsStore.setBlockFoldingEnabled(v)}
				label={t("terminal.toggle.blockFoldingEnabled", "Enable block folding")}
				hint={t(
					"terminal.hint.blockFoldingEnabled",
					"Allow collapsing a command block's output with Cmd+Shift+. or a gutter click.",
				)}
			/>
		</div>
	);
};
