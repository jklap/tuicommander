import { createEffect, onCleanup } from "solid-js";
import { registerAiChatContextActions } from "../components/AIChatPanel/contextMenuActions";
import { appLogger } from "../stores/appLogger";
import { contextMenuActionsStore } from "../stores/contextMenuActionsStore";
import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { settingsStore } from "../stores/settings";

interface PluginContextActionOptions {
	executeSmartPrompt: (prompt: SavedPrompt, manualVariables?: Record<string, string>) => Promise<unknown>;
	canExecute: (prompt: SavedPrompt) => { ok: boolean };
}

/** Owns context-action registration lifecycles for built-in smart prompts. */
export function usePluginContextActions(options: PluginContextActionOptions): void {
	// The AI Chat entries exist only while the panel they open does: an entry
	// that opens a panel the user cannot see is a dead end.
	createEffect(() => {
		if (!settingsStore.isAiChatEnabled()) return;
		const disposables = registerAiChatContextActions();
		onCleanup(() => disposables.forEach((disposable) => disposable.dispose()));
	});

	createEffect(() => {
		const disposables: Array<{ dispose(): void }> = [];
		for (const prompt of promptLibraryStore.getSmartByPlacement("git-branches")) {
			disposables.push(
				contextMenuActionsStore.registerContextAction("smart-prompts", {
					id: `smart:${prompt.id}`,
					label: prompt.name,
					target: "branch",
					action: (context) => {
						options
							.executeSmartPrompt(prompt, context.branchName ? { branch_name: context.branchName } : undefined)
							.catch((error) => appLogger.error("prompts", "Smart prompt execution failed", error));
					},
				}),
			);
		}
		onCleanup(() => disposables.forEach((disposable) => disposable.dispose()));
	});

	createEffect(() => {
		const disposables: Array<{ dispose(): void }> = [];
		for (const prompt of promptLibraryStore.getSmartByPlacement("terminal-context")) {
			disposables.push(
				contextMenuActionsStore.registerContextAction("smart-prompts", {
					id: `smart:${prompt.id}`,
					label: prompt.name,
					target: "terminal",
					action: () => {
						options
							.executeSmartPrompt(prompt)
							.catch((error) => appLogger.error("prompts", "Smart prompt execution failed", error));
					},
					disabled: () => !options.canExecute(prompt).ok,
				}),
			);
		}
		onCleanup(() => disposables.forEach((disposable) => disposable.dispose()));
	});
}
