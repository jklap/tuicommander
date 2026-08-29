import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { IconSwatchGrid } from "../../indicators/IconSwatchGrid";
import { ICON_IDS } from "../../indicators/icons";

describe("IconSwatchGrid", () => {
	let onSelect: (iconId: string) => void;

	beforeEach(() => {
		onSelect = vi.fn();
	});

	it("renders one swatch per curated icon id, each titled with its id", () => {
		const { container } = render(() => <IconSwatchGrid currentIconId="dot" onSelect={onSelect} />);
		const swatches = container.querySelectorAll(".swatch");
		expect(swatches.length).toBe(ICON_IDS.length);
		for (const iconId of ICON_IDS) {
			expect(container.querySelector(`[title="${iconId}"]`)).not.toBeNull();
		}
	});

	it("marks the current icon id's swatch active", () => {
		const { container } = render(() => <IconSwatchGrid currentIconId="diamond" onSelect={onSelect} />);
		const active = container.querySelector(".active");
		expect(active?.getAttribute("title")).toBe("diamond");
	});

	it("clicking an icon swatch calls onSelect with that icon's id", () => {
		const { getByTitle } = render(() => <IconSwatchGrid currentIconId="dot" onSelect={onSelect} />);
		fireEvent.click(getByTitle("ring"));
		expect(onSelect).toHaveBeenCalledWith("ring");
	});
});
