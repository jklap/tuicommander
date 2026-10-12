import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { LaunchInstructionsEditor } from "../LaunchInstructionsEditor";

const MARKERS = ["intent:", "suggest:", "💬 "];
const DEFAULT = [
	"## TUIC output markers",
	"- `intent: <desc> (<title>)`",
	"- `suggest: [ A | B | C ]`",
	"- `💬 ` answers",
].join("\n");

function renderEditor(value: string | undefined, onSave = vi.fn()) {
	const view = render(() => (
		<LaunchInstructionsEditor
			id="li"
			label="Launch instructions"
			value={value}
			fallback={DEFAULT}
			requiredMarkers={MARKERS}
			onSave={onSave}
		/>
	));
	return { ...view, onSave, textarea: view.getByLabelText("Launch instructions") as HTMLTextAreaElement };
}

describe("LaunchInstructionsEditor", () => {
	it("warns, naming the marker, when an edit drops the suggest line — and still saves it", () => {
		// Catches: removing the suggest: line saves silently without a warning that names the marker.
		const { textarea, onSave, queryByRole, getByRole } = renderEditor(undefined);
		expect(queryByRole("alert")).toBeNull();
		const edited = DEFAULT.split("\n")
			.filter((line) => !line.includes("suggest:"))
			.join("\n");
		fireEvent.input(textarea, { target: { value: edited } });
		const alert = getByRole("alert");
		expect(alert.textContent).toContain("suggest:");
		expect(alert.textContent).not.toContain("intent:");
		fireEvent.blur(textarea);
		expect(onSave).toHaveBeenCalledWith(edited);
	});

	it("treats an empty text as 'off', without a marker warning", () => {
		// Catches: the documented way to turn injection off shown as a broken text.
		const { textarea, onSave, queryByRole } = renderEditor(undefined);
		fireEvent.input(textarea, { target: { value: "" } });
		expect(queryByRole("alert")).toBeNull();
		fireEvent.blur(textarea);
		expect(onSave).toHaveBeenCalledWith("");
	});

	it("resets an override to the default and saves nothing for an unchanged blur", () => {
		// Catches: Reset storing the default text as an override, or a focus-out writing agents.json.
		const { textarea, onSave, getByText } = renderEditor("custom");
		expect(textarea.value).toBe("custom");
		fireEvent.blur(textarea);
		expect(onSave).not.toHaveBeenCalled();
		fireEvent.click(getByText("Reset to default"));
		expect(onSave).toHaveBeenCalledWith(undefined);
	});
});
