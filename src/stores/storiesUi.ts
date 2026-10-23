import { createSignal } from "solid-js";
import { togglePanel } from "../panelRouter";
import { uiStore } from "./ui";

const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);

export const storiesUi = {
	visible,
	project,
	open(path: string | null | undefined): void {
		if (!path) return;
		if (uiStore.isDetached("stories")) {
			togglePanel("stories");
			return;
		}
		setProject(path);
		setVisible(true);
	},
	close(): void {
		setVisible(false);
	},
};
