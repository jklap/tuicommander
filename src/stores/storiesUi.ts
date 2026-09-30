import { createSignal } from "solid-js";

const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);

export const storiesUi = {
	visible,
	project,
	open(path: string | null | undefined): void {
		if (!path) return;
		setProject(path);
		setVisible(true);
	},
	close(): void {
		setVisible(false);
	},
};
