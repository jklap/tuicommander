import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
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
	const [current, setCurrent] = createSignal(value);
	const save = (text: string | undefined) => {
		onSave(text);
		setCurrent(text);
	};
	const view = render(() => (
		<LaunchInstructionsEditor
			id="li"
			label="Launch instructions"
			value={current()}
			fallback={DEFAULT}
			requiredMarkers={MARKERS}
			onSave={save}
			inheritanceLabel="Inherits from global"
			customizeLabel="Customize for Claude"
			resetLabel="Use global"
		/>
	));
	return { ...view, onSave, textarea: view.queryByRole("textbox") as HTMLTextAreaElement };
}

describe("LaunchInstructionsEditor", () => {
	it("catches accidental overrides from inherited preview focus and blur", () => {
		const { queryByRole, getByText, getByLabelText, onSave } = renderEditor(undefined);
		expect(queryByRole("textbox")).toBeNull();
		expect(getByText("Inherits from global")).toBeTruthy();
		fireEvent.blur(getByLabelText("Launch instructions"));
		expect(onSave).not.toHaveBeenCalled();
	});

	it("catches implicit customization and reset freezing the inherited text", () => {
		const { getByRole, queryByRole, getByText, onSave } = renderEditor(undefined);
		fireEvent.click(getByRole("button", { name: "Customize for Claude" }));
		expect(onSave).toHaveBeenLastCalledWith("");
		expect((getByRole("textbox") as HTMLTextAreaElement).value).toBe("");
		fireEvent.click(getByText("Use global"));
		expect(onSave).toHaveBeenLastCalledWith(undefined);
		expect(queryByRole("textbox")).toBeNull();
	});

	it("catches additions being required to duplicate generated markers", () => {
		// Catches: user additions being rejected unless they copy generated markers.
		const { textarea, onSave, queryByRole } = renderEditor(DEFAULT);
		expect(queryByRole("alert")).toBeNull();
		const edited = DEFAULT.split("\n")
			.filter((line) => !line.includes("suggest:"))
			.join("\n");
		fireEvent.input(textarea, { target: { value: edited } });
		expect(queryByRole("alert")).toBeNull();
		fireEvent.blur(textarea);
		expect(onSave).toHaveBeenCalledWith(edited);
	});

	it("catches empty additions being treated as missing markers", () => {
		// Catches: empty additions being incorrectly shown as invalid instructions.
		const { textarea, onSave, queryByRole } = renderEditor(DEFAULT);
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
		fireEvent.click(getByText("Use global"));
		expect(onSave).toHaveBeenCalledWith(undefined);
	});
});
