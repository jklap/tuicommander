import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { trackPanelWidth } from "../../components/AIChatPanel/trackPanelWidth";
import { ToastContainer } from "../../components/ToastContainer/ToastContainer";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";

/**
 * Toasts are fixed to the viewport, so a docked AI Chat panel lies underneath
 * them. The panel's measured width is the single offset: the toast container
 * moves left by exactly that much, and returns when the width is 0 (panel
 * closed, or detached into a window of its own).
 */
describe("toast inset for the docked AI Chat panel", () => {
	beforeEach(() => {
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		uiStore.setAiChatPanelWidth(0);
	});
	afterEach(() => {
		cleanup();
		uiStore.setAiChatPanelWidth(0);
	});

	function container(): HTMLElement {
		toastsStore.add("hello", "", "info");
		const { container: root } = render(() => <ToastContainer />);
		return root.firstElementChild as HTMLElement;
	}

	it("offsets the container by the panel width", () => {
		uiStore.setAiChatPanelWidth(500);
		expect(container().style.getPropertyValue("--toast-right-inset")).toBe("500px");
	});

	it("applies no offset when no panel is docked", () => {
		expect(container().style.getPropertyValue("--toast-right-inset")).toBe("");
	});

	it("follows the width when the panel is resized or closed", () => {
		const el = container();
		uiStore.setAiChatPanelWidth(620);
		expect(el.style.getPropertyValue("--toast-right-inset")).toBe("620px");
		uiStore.setAiChatPanelWidth(0);
		expect(el.style.getPropertyValue("--toast-right-inset")).toBe("");
	});
});

describe("trackPanelWidth", () => {
	let callback: () => void;
	const disconnect = vi.fn();
	beforeEach(() => {
		disconnect.mockClear();
		vi.stubGlobal(
			"ResizeObserver",
			class {
				constructor(cb: () => void) {
					callback = cb;
				}
				observe() {}
				disconnect = disconnect;
			},
		);
	});
	afterEach(() => vi.unstubAllGlobals());

	it("reports the initial and every later width, and 0 on stop", () => {
		const widths: number[] = [];
		let current = 500;
		const el = document.createElement("div");
		el.getBoundingClientRect = () => ({ width: current }) as DOMRect;
		const stop = trackPanelWidth(el, (w) => widths.push(w));
		current = 0; // display:none when the panel closes
		callback();
		current = 640;
		callback();
		stop();
		expect(widths).toEqual([500, 0, 640, 0]);
		expect(disconnect).toHaveBeenCalled();
	});
});
