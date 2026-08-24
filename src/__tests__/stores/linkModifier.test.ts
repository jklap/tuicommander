import { afterEach, describe, expect, it } from "vitest";
import { initLinkModifier, linkModifierHeld } from "../../stores/linkModifier";

function mockPlatform(value: string) {
	Object.defineProperty(navigator, "platform", { value, writable: true, configurable: true });
}

describe("linkModifier store", () => {
	const originalPlatform = Object.getOwnPropertyDescriptor(navigator, "platform");

	afterEach(() => {
		if (originalPlatform) Object.defineProperty(navigator, "platform", originalPlatform);
		// Leave the signal in a clean state for the next test.
		window.dispatchEvent(new Event("blur"));
	});

	it("is idempotent across multiple init calls", () => {
		initLinkModifier();
		initLinkModifier();
		mockPlatform("MacIntel");
		document.dispatchEvent(new KeyboardEvent("keydown", { metaKey: true }));
		expect(linkModifierHeld()).toBe(true);
		document.dispatchEvent(new KeyboardEvent("keyup", { metaKey: false }));
		expect(linkModifierHeld()).toBe(false);
	});

	it("tracks Cmd on macOS", () => {
		initLinkModifier();
		mockPlatform("MacIntel");
		document.dispatchEvent(new KeyboardEvent("keydown", { metaKey: true, ctrlKey: false }));
		expect(linkModifierHeld()).toBe(true);
		document.dispatchEvent(new KeyboardEvent("keydown", { metaKey: false, ctrlKey: true }));
		expect(linkModifierHeld()).toBe(false);
	});

	it("tracks Ctrl on Windows/Linux", () => {
		initLinkModifier();
		mockPlatform("Win32");
		document.dispatchEvent(new KeyboardEvent("keydown", { ctrlKey: true, metaKey: false }));
		expect(linkModifierHeld()).toBe(true);
		document.dispatchEvent(new KeyboardEvent("keyup", { ctrlKey: false }));
		expect(linkModifierHeld()).toBe(false);
	});

	it("resets on window blur so a stuck hold can't survive focus loss", () => {
		initLinkModifier();
		mockPlatform("MacIntel");
		document.dispatchEvent(new KeyboardEvent("keydown", { metaKey: true }));
		expect(linkModifierHeld()).toBe(true);
		window.dispatchEvent(new Event("blur"));
		expect(linkModifierHeld()).toBe(false);
	});

	it("resets on visibilitychange", () => {
		initLinkModifier();
		mockPlatform("MacIntel");
		document.dispatchEvent(new KeyboardEvent("keydown", { metaKey: true }));
		expect(linkModifierHeld()).toBe(true);
		document.dispatchEvent(new Event("visibilitychange"));
		expect(linkModifierHeld()).toBe(false);
	});
});
