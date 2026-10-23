import { createSignal } from "solid-js";
import { togglePanel } from "../panelRouter";
import type { StoryType, StoryVisibility } from "../types/stories.gen";
import { uiStore } from "./ui";

const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);

const [visibility, setVisibility] = createSignal<StoryVisibility>("all");
const [storyType, setStoryType] = createSignal<StoryType | "all" | "untyped">("all");
const [view, setView] = createSignal<"list" | "kanban">("list");

export const storiesUi = {
	visibility,
	setVisibility,
	storyType,
	setStoryType,
	view,
	setView,
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
