import { createMemo, createResource } from "solid-js";
import { invoke } from "../../invoke";
import type { PluginSettingAnnotation, PluginSettingsSchema } from "../../plugins/pluginSettings";
import { pluginStore } from "../../stores/pluginStore";

/** Wire projection of the Rust settings catalogue, not a manifest-schema traversal. */
export interface PluginSettingsField extends PluginSettingAnnotation {
	id: string;
	path: string;
	label: string;
	order: number;
	schema: PluginSettingsSchema;
}
export interface PluginSettingsPage {
	id: string;
	label: string;
	fields: PluginSettingsField[];
}
export const pluginFieldLabel = (field: PluginSettingsField): string => field.schema.title ?? field.label;

export function createPluginSettingsCatalogue() {
	const loaded = createMemo(() => pluginStore.getAll().filter((plugin) => plugin.loaded));
	const [catalogue] = createResource(loaded, async (plugins) =>
		Promise.all(
			plugins.map(async (plugin): Promise<PluginSettingsPage | null> => {
				try {
					const reply = await invoke<{ fields: Record<string, PluginSettingsField> }>("plugin_settings_action", {
						pluginId: plugin.id,
						action: { action: "describe" },
					});
					const fields = Object.values(reply.fields).sort((a, b) => a.order - b.order);
					return fields.length ? { id: plugin.id, label: plugin.manifest?.name ?? plugin.id, fields } : null;
				} catch {
					// Plugins without registered settings have no settings page.
					return null;
				}
			}),
		),
	);
	// Resource retains its last value during reload. Never retain an unloaded editor.
	return () =>
		(catalogue() ?? []).filter(
			(page): page is PluginSettingsPage => page !== null && loaded().some((plugin) => plugin.id === page.id),
		);
}
