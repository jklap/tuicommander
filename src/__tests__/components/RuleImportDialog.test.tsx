import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { RuleImportDialog } from "../../components/RuleImportDialog/RuleImportDialog";
import type { SmartSelectionRule } from "../../components/Terminal/smartSelectionTypes";
import type { RuleImportCandidate } from "../../utils/smartSelectionExport";

function makeRule(overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule {
	return {
		id: "r1",
		name: "Word",
		regex: "[\\w-]+",
		precision: "normal",
		enabled: true,
		actions: [],
		...overrides,
	};
}

function makeCandidates(): RuleImportCandidate[] {
	return [
		{ rule: makeRule({ id: "r1", name: "Word" }), status: "conflict", needsReview: false },
		{ rule: makeRule({ id: "r2", name: "My Custom Rule" }), status: "new", needsReview: false },
		{
			rule: makeRule({
				id: "r3",
				name: "Prune Branches",
				actions: [{ kind: "run_command", title: "Run", parameter: "git branch -D \\0", isDefault: false }],
			}),
			status: "new",
			needsReview: true,
		},
	];
}

const defaultProps = () => ({
	candidates: makeCandidates(),
	warnings: [],
	willMaterializeDefaults: false,
	isBuiltIn: (id: string) => id === "r1",
	onImport: vi.fn(),
	onCancel: vi.fn(),
});

describe("RuleImportDialog", () => {
	it("renders one row per candidate with status badges", () => {
		const { getByTestId } = render(() => <RuleImportDialog {...defaultProps()} />);
		expect(getByTestId("import-check-r1")).toBeTruthy();
		expect(getByTestId("import-check-r2")).toBeTruthy();
		expect(getByTestId("import-check-r3")).toBeTruthy();
	});

	it("shows a review warning for rules that run commands or send text", () => {
		const { getByTestId, queryByTestId } = render(() => <RuleImportDialog {...defaultProps()} />);
		expect(getByTestId("import-warning-r3")).toBeTruthy();
		expect(queryByTestId("import-warning-r1")).toBeNull();
	});

	it("defaults to everything selected", () => {
		const { getByTestId } = render(() => <RuleImportDialog {...defaultProps()} />);
		expect(getByTestId("import-confirm-btn").textContent).toContain("3");
	});

	it("None deselects all, All reselects all", () => {
		const { getByText, getByTestId } = render(() => <RuleImportDialog {...defaultProps()} />);
		fireEvent.click(getByText("None"));
		expect(getByTestId("import-confirm-btn").textContent).toContain("0");
		expect((getByTestId("import-confirm-btn") as HTMLButtonElement).disabled).toBe(true);

		fireEvent.click(getByText("All"));
		expect(getByTestId("import-confirm-btn").textContent).toContain("3");
	});

	it("New only selects only new-status candidates", () => {
		const { getByText, getByTestId } = render(() => <RuleImportDialog {...defaultProps()} />);
		fireEvent.click(getByText("New only"));
		expect((getByTestId("import-check-r1") as HTMLInputElement).checked).toBe(false);
		expect((getByTestId("import-check-r2") as HTMLInputElement).checked).toBe(true);
		expect((getByTestId("import-check-r3") as HTMLInputElement).checked).toBe(true);
	});

	it("Import calls onImport with only the checked ids", () => {
		const props = defaultProps();
		const { getByTestId } = render(() => <RuleImportDialog {...props} />);
		fireEvent.click(getByTestId("import-check-r1"));
		fireEvent.click(getByTestId("import-confirm-btn"));
		expect(props.onImport).toHaveBeenCalledWith(["r2", "r3"]);
	});

	it("Cancel calls onCancel", () => {
		const props = defaultProps();
		const { getByTestId } = render(() => <RuleImportDialog {...props} />);
		fireEvent.click(getByTestId("import-cancel-btn"));
		expect(props.onCancel).toHaveBeenCalledOnce();
	});

	it("renders parse warnings when present", () => {
		const props = { ...defaultProps(), warnings: ['Rule "Foo" has a duplicate id and was skipped'] };
		const { getByTestId } = render(() => <RuleImportDialog {...props} />);
		expect(getByTestId("import-parse-warnings").textContent).toContain("duplicate id");
	});

	it("uses singular wording for a single candidate", () => {
		const props = {
			...defaultProps(),
			candidates: [{ rule: makeRule({ id: "r1" }), status: "new" as const, needsReview: false }],
		};
		const { container } = render(() => <RuleImportDialog {...props} />);
		expect(container.textContent).toContain("1 rule in file");
		expect(container.textContent).not.toContain("1 rules in file");
	});

	it("notes that restoring built-in defaults remains available for a conflicting built-in rule", () => {
		const props = {
			...defaultProps(),
			candidates: [{ rule: makeRule({ id: "r1" }), status: "conflict" as const, needsReview: false }],
			isBuiltIn: () => true,
		};
		const { container } = render(() => <RuleImportDialog {...props} />);
		expect(container.textContent).toContain("Restore built-in defaults");
	});

	it("notes a plain replacement for a conflicting custom rule", () => {
		const props = {
			...defaultProps(),
			candidates: [{ rule: makeRule({ id: "custom-1" }), status: "conflict" as const, needsReview: false }],
			isBuiltIn: () => false,
		};
		const { container } = render(() => <RuleImportDialog {...props} />);
		expect(container.textContent).toContain("Replaces your current version of this rule.");
	});

	it("shows the materialize-defaults footnote only when the stored rule list is empty", () => {
		const { queryByTestId, unmount } = render(() => (
			<RuleImportDialog {...defaultProps()} willMaterializeDefaults={false} />
		));
		expect(queryByTestId("import-footnote")).toBeNull();
		unmount();

		const { getByTestId } = render(() => <RuleImportDialog {...defaultProps()} willMaterializeDefaults={true} />);
		expect(getByTestId("import-footnote").textContent).toContain("Importing saves a copy of all built-in rules");
	});
});
