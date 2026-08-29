import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { AnimationOptionList } from "../../indicators/AnimationOptionList";
import { ANIMATION_LABELS, INDICATOR_ANIMATIONS } from "../../indicators/animations";

describe("AnimationOptionList", () => {
	let onSelect: (animationId: string) => void;

	beforeEach(() => {
		onSelect = vi.fn();
	});

	it("renders every animation option by default, with its label and live preview animation", () => {
		const { container } = render(() => <AnimationOptionList currentAnimationId="none" onSelect={onSelect} />);
		const rows = Array.from(container.querySelectorAll(".row")) as HTMLElement[];
		expect(rows.length).toBe(Object.keys(ANIMATION_LABELS).length);
		for (const [id, label] of Object.entries(ANIMATION_LABELS)) {
			expect(container.textContent).toContain(label);
			const preview = rows.find((r) => r.textContent?.includes(label))?.querySelector(".preview") as HTMLElement | null;
			expect(preview?.style.animation).toBe(INDICATOR_ANIMATIONS[id as keyof typeof INDICATOR_ANIMATIONS]);
		}
	});

	it("narrows to allowedAnimationIds when given", () => {
		const { queryByText } = render(() => (
			<AnimationOptionList
				currentAnimationId="none"
				allowedAnimationIds={["none", "pulse", "breathe"]}
				onSelect={onSelect}
			/>
		));
		expect(queryByText("Pulse")).toBeTruthy();
		expect(queryByText("Breathe (dim → solid → dim)")).toBeTruthy();
		expect(queryByText("Glow")).toBeNull();
		expect(queryByText("Spin")).toBeNull();
	});

	it("marks the current animation id's row active", () => {
		const { container, getByText } = render(() => (
			<AnimationOptionList currentAnimationId="blink" onSelect={onSelect} />
		));
		const blinkRow = getByText("Blink").closest(".row");
		expect(blinkRow?.className).toContain("active");
		expect(container.querySelectorAll(".active").length).toBe(1);
	});

	it("clicking an option calls onSelect with that animation's id", () => {
		const { getByText } = render(() => <AnimationOptionList currentAnimationId="none" onSelect={onSelect} />);
		fireEvent.click(getByText("Blink"));
		expect(onSelect).toHaveBeenCalledWith("blink");
	});
});
