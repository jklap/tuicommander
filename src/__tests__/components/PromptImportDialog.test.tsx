import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { PromptImportDialog } from "../../components/PromptImportDialog/PromptImportDialog";
import type { SavedPrompt } from "../../stores/promptLibrary";
import type { ImportCandidate } from "../../utils/promptExport";

function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "p1",
		name: "Smart Commit",
		content: "c",
		category: "custom",
		isFavorite: false,
		createdAt: 1,
		updatedAt: 1,
		...overrides,
	};
}

function makeCandidates(): ImportCandidate[] {
	return [
		{ prompt: makePrompt({ id: "p1", name: "Smart Commit" }), status: "conflict", needsReview: false },
		{ prompt: makePrompt({ id: "p2", name: "My Deploy Script" }), status: "new", needsReview: false },
		{
			prompt: makePrompt({ id: "p3", name: "Prune Branches", executionMode: "shell" }),
			status: "new",
			needsReview: true,
		},
	];
}

const defaultProps = () => ({
	candidates: makeCandidates(),
	warnings: [],
	onImport: vi.fn(),
	onCancel: vi.fn(),
});

describe("PromptImportDialog", () => {
	it("renders one row per candidate with status badges", () => {
		const { getByTestId } = render(() => <PromptImportDialog {...defaultProps()} />);
		expect(getByTestId("import-check-p1")).toBeTruthy();
		expect(getByTestId("import-check-p2")).toBeTruthy();
		expect(getByTestId("import-check-p3")).toBeTruthy();
	});

	it("shows a review warning for shell/api prompts", () => {
		const { getByTestId, queryByTestId } = render(() => <PromptImportDialog {...defaultProps()} />);
		expect(getByTestId("import-warning-p3")).toBeTruthy();
		expect(queryByTestId("import-warning-p1")).toBeNull();
	});

	it("defaults to everything selected", () => {
		const { getByTestId } = render(() => <PromptImportDialog {...defaultProps()} />);
		expect(getByTestId("import-confirm-btn").textContent).toContain("3");
	});

	it("None deselects all, All reselects all", () => {
		const { getByText, getByTestId } = render(() => <PromptImportDialog {...defaultProps()} />);
		fireEvent.click(getByText("None"));
		expect(getByTestId("import-confirm-btn").textContent).toContain("0");
		expect((getByTestId("import-confirm-btn") as HTMLButtonElement).disabled).toBe(true);

		fireEvent.click(getByText("All"));
		expect(getByTestId("import-confirm-btn").textContent).toContain("3");
	});

	it("New only selects only new-status candidates", () => {
		const { getByText, getByTestId } = render(() => <PromptImportDialog {...defaultProps()} />);
		fireEvent.click(getByText("New only"));
		expect((getByTestId("import-check-p1") as HTMLInputElement).checked).toBe(false);
		expect((getByTestId("import-check-p2") as HTMLInputElement).checked).toBe(true);
		expect((getByTestId("import-check-p3") as HTMLInputElement).checked).toBe(true);
	});

	it("Import calls onImport with only the checked ids", () => {
		const props = defaultProps();
		const { getByTestId } = render(() => <PromptImportDialog {...props} />);
		fireEvent.click(getByTestId("import-check-p1"));
		fireEvent.click(getByTestId("import-confirm-btn"));
		expect(props.onImport).toHaveBeenCalledWith(["p2", "p3"]);
	});

	it("unchecking then rechecking a candidate restores it to the selection", () => {
		const props = defaultProps();
		const { getByTestId } = render(() => <PromptImportDialog {...props} />);
		fireEvent.click(getByTestId("import-check-p1"));
		fireEvent.click(getByTestId("import-check-p1"));
		fireEvent.click(getByTestId("import-confirm-btn"));
		expect(props.onImport).toHaveBeenCalledWith(["p1", "p2", "p3"]);
	});

	it("Cancel calls onCancel", () => {
		const props = defaultProps();
		const { getByTestId } = render(() => <PromptImportDialog {...props} />);
		fireEvent.click(getByTestId("import-cancel-btn"));
		expect(props.onCancel).toHaveBeenCalledOnce();
	});

	it("renders parse warnings when present", () => {
		const props = { ...defaultProps(), warnings: ["Entry 4 is missing an id and was skipped"] };
		const { getByTestId } = render(() => <PromptImportDialog {...props} />);
		expect(getByTestId("import-parse-warnings").textContent).toContain("Entry 4");
	});

	it("uses singular wording for a single candidate", () => {
		const props = {
			...defaultProps(),
			candidates: [{ prompt: makePrompt({ id: "p1" }), status: "new" as const, needsReview: false }],
		};
		const { container } = render(() => <PromptImportDialog {...props} />);
		expect(container.textContent).toContain("1 prompt in file");
		expect(container.textContent).not.toContain("1 prompts in file");
	});

	it("notes that reset-to-default remains available for a conflicting built-in", () => {
		const props = {
			...defaultProps(),
			candidates: [
				{
					prompt: makePrompt({ id: "p1", builtIn: true }),
					status: "conflict" as const,
					needsReview: false,
				},
			],
		};
		const { container } = render(() => <PromptImportDialog {...props} />);
		expect(container.textContent).toContain("Reset to default remains available");
	});
});
