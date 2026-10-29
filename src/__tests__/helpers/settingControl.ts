/** A settings control's form element by its catalogue id.
 *
 * Main's `SettingRow` layout puts the label in a separate column from the
 * control, so label adjacency is not a stable handle; the catalogue id is, and
 * looking it up fails loudly when a control loses its binding. The row wrapper
 * carries the id too, so this matches the form element itself. */
export function settingControl<T extends HTMLElement = HTMLInputElement>(container: ParentNode, settingId: string): T {
	const el = container.querySelector<T>(
		["input", "select", "textarea"].map((tag) => `${tag}[data-setting-id="${settingId}"]`).join(", "),
	);
	if (!el) throw new Error(`No control found for setting "${settingId}"`);
	return el;
}
