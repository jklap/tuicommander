import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockAgentConfigsStore = vi.hoisted(() => ({
	getHeadlessAgent: vi.fn<() => string | null>(() => null),
	setHeadlessAgent: vi.fn(),
	getRunConfigs: vi.fn<(type: string) => unknown[]>(() => []),
}));

vi.mock("../../../stores/agentConfigs", () => ({
	agentConfigsStore: mockAgentConfigsStore,
}));

import { HeadlessAgentSelect } from "../../../components/SettingsPanel/HeadlessAgentSelect";

describe("HeadlessAgentSelect", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockAgentConfigsStore.getHeadlessAgent.mockReturnValue(null);
		mockAgentConfigsStore.getRunConfigs.mockReturnValue([]);
	});

	it("always offers '— Not configured —' and 'External API', selecting neither by default", () => {
		const { container } = render(() => <HeadlessAgentSelect agentTypes={[]} />);
		const select = container.querySelector("select") as HTMLSelectElement;
		const optionTexts = Array.from(select.options).map((o) => o.textContent);
		expect(optionTexts).toEqual(["— Not configured —", "External API"]);
		expect(select.value).toBe("");
	});

	it("shows an agent with no run configs as a plain option, not an optgroup", () => {
		const { container } = render(() => <HeadlessAgentSelect agentTypes={["claude"]} />);
		const select = container.querySelector("select") as HTMLSelectElement;
		expect(select.querySelector("optgroup")).toBeNull();
		expect(Array.from(select.options).some((o) => o.value === "claude" && o.textContent === "Claude Code")).toBe(true);
	});

	it("groups an agent with run configs under an optgroup, including a default row for the bare agent", () => {
		mockAgentConfigsStore.getRunConfigs.mockImplementation((type: string) =>
			type === "claude" ? [{ name: "My Config", command: "claude", args: [], env: {}, is_default: false }] : [],
		);
		const { container } = render(() => <HeadlessAgentSelect agentTypes={["claude"]} />);
		const select = container.querySelector("select") as HTMLSelectElement;
		const optgroup = select.querySelector("optgroup");
		expect(optgroup?.getAttribute("label")).toBe("Claude Code");
		expect(select.querySelector('option[value="claude:My Config"]')?.textContent).toBe("My Config");
	});

	it("selects the option matching the currently persisted value, including a composite run-config value", () => {
		mockAgentConfigsStore.getHeadlessAgent.mockReturnValue("claude:My Config");
		mockAgentConfigsStore.getRunConfigs.mockReturnValue([
			{ name: "My Config", command: "claude", args: [], env: {}, is_default: false },
		]);
		const { container } = render(() => <HeadlessAgentSelect agentTypes={["claude"]} />);
		const select = container.querySelector("select") as HTMLSelectElement;
		expect(select.value).toBe("claude:My Config");
	});

	it("calls setHeadlessAgent with the selected value, or null for '— Not configured —'", () => {
		const { container } = render(() => <HeadlessAgentSelect agentTypes={["claude"]} />);
		const select = container.querySelector("select") as HTMLSelectElement;

		fireEvent.change(select, { target: { value: "claude" } });
		expect(mockAgentConfigsStore.setHeadlessAgent).toHaveBeenLastCalledWith("claude");

		fireEvent.change(select, { target: { value: "" } });
		expect(mockAgentConfigsStore.setHeadlessAgent).toHaveBeenLastCalledWith(null);

		fireEvent.change(select, { target: { value: "api" } });
		expect(mockAgentConfigsStore.setHeadlessAgent).toHaveBeenLastCalledWith("api");
	});

	it("renders one optgroup per agent type, in the order given, when multiple agents have run configs", () => {
		mockAgentConfigsStore.getRunConfigs.mockImplementation((type: string) => [
			{ name: `${type} cfg`, command: type, args: [], env: {}, is_default: true },
		]);
		const { container } = render(() => <HeadlessAgentSelect agentTypes={["claude", "codex"]} />);
		const select = container.querySelector("select") as HTMLSelectElement;
		const labels = Array.from(select.querySelectorAll("optgroup")).map((g) => g.getAttribute("label"));
		expect(labels).toEqual(["Claude Code", "Codex CLI"]);
	});
});
