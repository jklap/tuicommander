import { describe, expect, it } from "vitest";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { sanitizePrompt } from "../../utils/promptSanitize";

function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "p1",
		name: "Test Prompt",
		content: "hello",
		category: "custom",
		isFavorite: false,
		createdAt: 1,
		updatedAt: 1,
		...overrides,
	};
}

describe("sanitizePrompt", () => {
	it("passes a valid prompt through untouched", () => {
		const prompt = makePrompt({ executionMode: "headless", injectTarget: "terminal", placement: ["toolbar"] });
		const { prompt: result, warnings, migratedPlacement } = sanitizePrompt(prompt);
		expect(result).toEqual(prompt);
		expect(warnings).toEqual([]);
		expect(migratedPlacement).toBe(false);
	});

	it("resets an invalid executionMode to inject", () => {
		// biome-ignore lint/suspicious/noExplicitAny: deliberately invalid input under test
		const prompt = makePrompt({ executionMode: "delete-everything" as any });
		const { prompt: result, warnings } = sanitizePrompt(prompt);
		expect(result.executionMode).toBe("inject");
		expect(warnings.some((w) => w.includes("invalid executionMode"))).toBe(true);
	});

	it("strips an invalid injectTarget", () => {
		// biome-ignore lint/suspicious/noExplicitAny: deliberately invalid input under test
		const prompt = makePrompt({ injectTarget: "somewhere-else" as any });
		const { prompt: result, warnings } = sanitizePrompt(prompt);
		expect(result.injectTarget).toBeUndefined();
		expect(warnings.some((w) => w.includes("invalid injectTarget"))).toBe(true);
	});

	it("strips an unknown preferredAgent", () => {
		// biome-ignore lint/suspicious/noExplicitAny: deliberately invalid input under test
		const prompt = makePrompt({ preferredAgent: "not-a-real-agent" as any });
		const { prompt: result, warnings } = sanitizePrompt(prompt);
		expect(result.preferredAgent).toBeUndefined();
		expect(warnings.some((w) => w.includes("invalid preferredAgent"))).toBe(true);
	});

	it("drops a non-array placement", () => {
		// biome-ignore lint/suspicious/noExplicitAny: deliberately invalid input under test
		const prompt = makePrompt({ placement: "toolbar" as any });
		const { prompt: result } = sanitizePrompt(prompt);
		expect(result.placement).toBeUndefined();
	});

	it("migrates the legacy tab-context placement to terminal-context", () => {
		// biome-ignore lint/suspicious/noExplicitAny: exercising the legacy placement name
		const prompt = makePrompt({ placement: ["tab-context", "toolbar"] as any });
		const { prompt: result, migratedPlacement } = sanitizePrompt(prompt);
		expect(result.placement).toEqual(["terminal-context", "toolbar"]);
		expect(migratedPlacement).toBe(true);
	});

	it("leaves valid preferredAgent values alone", () => {
		const prompt = makePrompt({ preferredAgent: "claude" });
		const { prompt: result, warnings } = sanitizePrompt(prompt);
		expect(result.preferredAgent).toBe("claude");
		expect(warnings).toEqual([]);
	});
});
