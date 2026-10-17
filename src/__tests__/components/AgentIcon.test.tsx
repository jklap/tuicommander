import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { AgentIcon, getAgentIconSvg } from "../../components/ui/AgentIcon";

const pathOf = (agent: Parameters<typeof AgentIcon>[0]["agent"], provider?: string | null) => {
	const { container } = render(() => <AgentIcon agent={agent} provider={provider} />);
	return container.querySelector("path")?.getAttribute("d") ?? null;
};

describe("AgentIcon for ego sessions", () => {
	// TEST-CHANGE: Boss 2026-10-16 wants ego, not the provider (replaces story 1736 expectations).
	// Catches: an ego session being mistaken for Codex or another provider's agent.
	it.each(["openai-codex", "openai", "anthropic", "gemini", "github-copilot", "xai", "openrouter"])(
		"ego on %s keeps the ego icon instead of the provider icon",
		(provider) => {
			const d = pathOf("ego", provider);
			expect(d).not.toBeNull();
			expect(d).toBe(pathOf("ego"));
			expect(d).not.toBe(pathOf("codex"));
		},
	);

	// Catches: an unknown or missing provider rendering nothing or another provider's icon.
	it("keeps the ego icon with an unknown or missing provider", () => {
		const generic = pathOf("ego", "something-else");
		expect(generic).not.toBeNull();
		expect(pathOf("ego")).toBe(generic);
		expect(pathOf("ego", null)).toBe(generic);
		expect(generic).not.toBe(pathOf("claude"));
	});

	// Catches: a provider leaking onto a non-ego agent.
	it.each(["codex", "claude", "gemini", "amp", "cursor", "droid", "opencode"] as const)(
		"ignores the provider for %s",
		(agent) => {
			expect(pathOf(agent, "openrouter")).toBe(pathOf(agent));
		},
	);

	// Catches: inline notification SVGs still identifying ego by its provider.
	it("keeps the ego icon in the inline svg too", () => {
		expect(getAgentIconSvg("ego", 14, "anthropic")).toBe(getAgentIconSvg("ego", 14));
		expect(getAgentIconSvg("ego", 14, "github-copilot")).toContain('fill-rule="evenodd"');
	});
});
