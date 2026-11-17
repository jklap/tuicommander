import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockSetRules, mockToastAdd, mockState, shipped } = vi.hoisted(() => ({
	mockSetRules: vi.fn(),
	mockToastAdd: vi.fn(),
	mockState: { smartSelectionRules: [] as unknown[] },
	shipped: [] as unknown[],
}));

vi.mock("../../stores/settings", () => ({
	settingsStore: { state: mockState, setSmartSelectionRules: mockSetRules },
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: mockToastAdd } }));
vi.mock("../../components/Terminal/smartSelectionDefaults", () => ({
	get DEFAULT_SMART_SELECTION_RULES() {
		return shipped;
	},
}));

import { ruleContentHash } from "../../components/Terminal/smartSelectionReconcile";
import type { SmartSelectionRule } from "../../components/Terminal/smartSelectionTypes";
import { applyBuiltInRuleUpdates } from "../../utils/smartSelectionBuiltInUpdate";

const rule = (overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule => ({
	id: "dev-thing",
	name: "Thing",
	regex: "thing",
	precision: "normal",
	enabled: true,
	actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
	...overrides,
});
const v1 = rule();
const v2 = rule({ regex: "thing+", name: "Thing 2" });
const stamped = (r: SmartSelectionRule, extra: Partial<SmartSelectionRule> = {}) => ({
	...r,
	builtInHash: ruleContentHash(r),
	...extra,
});

describe("applyBuiltInRuleUpdates", () => {
	const openSettings = vi.fn();

	beforeEach(() => {
		vi.clearAllMocks();
		shipped.length = 0;
		shipped.push(v2);
		mockState.smartSelectionRules = [];
	});

	it("does nothing, and does not save, when there are no saved rules", () => {
		applyBuiltInRuleUpdates(openSettings);
		expect(mockSetRules).not.toHaveBeenCalled();
		expect(mockToastAdd).not.toHaveBeenCalled();
	});

	it("does nothing when everything is already up to date", () => {
		mockState.smartSelectionRules = [stamped(v2)];
		applyBuiltInRuleUpdates(openSettings);
		expect(mockSetRules).not.toHaveBeenCalled();
		expect(mockToastAdd).not.toHaveBeenCalled();
	});

	it("saves silently when it only stamped a rule saved before versions existed", () => {
		mockState.smartSelectionRules = [v2];
		applyBuiltInRuleUpdates(openSettings);
		expect(mockSetRules).toHaveBeenCalledTimes(1);
		expect(mockSetRules.mock.calls[0][0]).toEqual([stamped(v2)]);
		expect(mockToastAdd).not.toHaveBeenCalled();
	});

	it("updates an untouched rule, saves, and toasts with a link to the rules", () => {
		mockState.smartSelectionRules = [stamped(v1)];
		applyBuiltInRuleUpdates(openSettings);

		expect(mockSetRules).toHaveBeenCalledTimes(1);
		expect(mockSetRules.mock.calls[0][0]).toEqual([stamped(v2)]);
		expect(mockToastAdd).toHaveBeenCalledTimes(1);
		const [title, message, level, , action] = mockToastAdd.mock.calls[0];
		expect(title).toBe("Smart Selection rules updated");
		expect(level).toBe("info");
		expect(message).toContain("Thing 2");
		expect(message).toContain("Updated");
		expect(action.label).toBe("Go to the rules");

		action.onClick();
		expect(openSettings).toHaveBeenCalledWith("selection");
	});

	it("for an edited rule: keeps theirs, adds the new version disabled, and says so", () => {
		const mine = stamped(v1, { regex: "mine" });
		mockState.smartSelectionRules = [mine];
		applyBuiltInRuleUpdates(openSettings);

		const saved = mockSetRules.mock.calls[0][0] as SmartSelectionRule[];
		expect(saved).toHaveLength(2);
		expect(saved[0].regex).toBe("mine");
		expect(saved[1].name).toBe("Thing 2 (updated)");
		expect(saved[1].enabled).toBe(false);
		const message = mockToastAdd.mock.calls[0][1] as string;
		expect(message).toContain("You had edited Thing 2");
		expect(message).toContain("disabled");
		expect(message).not.toContain("Updated to the new built-in version");
	});

	it("mentions both outcomes in one toast when both happened", () => {
		const a1 = rule({ id: "dev-a", name: "A" });
		const a2 = rule({ id: "dev-a", name: "A", regex: "a2" });
		const b1 = rule({ id: "dev-b", name: "B" });
		const b2 = rule({ id: "dev-b", name: "B", regex: "b2" });
		shipped.length = 0;
		shipped.push(a2, b2);
		mockState.smartSelectionRules = [stamped(a1), stamped(b1, { regex: "mine" })];

		applyBuiltInRuleUpdates(openSettings);

		expect(mockToastAdd).toHaveBeenCalledTimes(1);
		const message = mockToastAdd.mock.calls[0][1] as string;
		expect(message).toContain("Updated to the new built-in version: A");
		expect(message).toContain("You had edited B");
	});

	it("is a no-op the second time (the saved result needs no further changes)", () => {
		mockState.smartSelectionRules = [stamped(v1)];
		applyBuiltInRuleUpdates(openSettings);
		mockState.smartSelectionRules = mockSetRules.mock.calls[0][0];
		mockSetRules.mockClear();
		mockToastAdd.mockClear();

		applyBuiltInRuleUpdates(openSettings);

		expect(mockSetRules).not.toHaveBeenCalled();
		expect(mockToastAdd).not.toHaveBeenCalled();
	});
});
