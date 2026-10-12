import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { AgentIcon, getAgentIconSvg } from "../../components/ui/AgentIcon";

const pathOf = (agent: Parameters<typeof AgentIcon>[0]["agent"], provider?: string | null) => {
	const { container } = render(() => <AgentIcon agent={agent} provider={provider} />);
	return container.querySelector("path")?.getAttribute("d") ?? null;
};

describe("AgentIcon for ego sessions", () => {
	// Catches: ego detected but mapped to no icon (letter fallback instead of an svg).
	it.each([
		["anthropic", "claude"],
		["openai", "codex"],
		["openai-codex", "codex"],
		["gemini", "gemini"],
	] as const)("ego on %s reuses the %s icon", (provider, agent) => {
		const d = pathOf("ego", provider);
		expect(d).not.toBeNull();
		expect(d).toBe(pathOf(agent));
	});

	// Catches: the new provider icons collapsing onto each other or onto the generic one.
	it("gives copilot, xai and openrouter each their own icon", () => {
		const ds = ["github-copilot", "xai", "openrouter", "something-else"].map((p) => pathOf("ego", p));
		expect(ds.every((d) => d !== null)).toBe(true);
		expect(new Set(ds).size).toBe(4);
	});

	// Catches: an unknown or missing provider rendering nothing or another provider's icon.
	it("falls back to the generic ego icon", () => {
		const generic = pathOf("ego", "something-else");
		expect(pathOf("ego")).toBe(generic);
		expect(pathOf("ego", null)).toBe(generic);
		expect(generic).not.toBe(pathOf("claude"));
	});

	// Catches: a provider leaking onto a non-ego agent.
	it("ignores the provider for other agents", () => {
		expect(pathOf("claude", "openrouter")).toBe(pathOf("claude"));
	});

	it("applies the provider to the inline svg too", () => {
		expect(getAgentIconSvg("ego", 14, "anthropic")).toBe(getAgentIconSvg("claude", 14));
		expect(getAgentIconSvg("ego", 14, "github-copilot")).toContain('fill-rule="evenodd"');
	});
});
