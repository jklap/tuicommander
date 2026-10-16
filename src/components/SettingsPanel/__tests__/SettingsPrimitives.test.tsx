import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it } from "vitest";
import { Input } from "../../ui/Input";
import { DangerZone, SectionCard, SettingRow, StatusSummary } from "../SettingsPrimitives";

describe("Settings presentation contract", () => {
	// Catches: row restyling loses the input label or leaves validation inaccessible.
	it("associates the existing control with its label, hint and changing error", () => {
		const [error, setError] = createSignal<string>();
		const [value, setValue] = createSignal("draft");
		const view = render(() => (
			<SectionCard title="Workspace">
				<SettingRow label="Folder" hint="New runs only" error={error()}>
					{(id, descriptionId) => (
						<Input
							id={id}
							aria-describedby={descriptionId()}
							value={value()}
							onInput={(event) => setValue(event.currentTarget.value)}
						/>
					)}
				</SettingRow>
			</SectionCard>
		));
		const input = view.getByLabelText("Folder") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "kept draft" } });
		setError("Could not save folder");
		expect(input.value).toBe("kept draft");
		const descriptions = input
			.getAttribute("aria-describedby")!
			.split(" ")
			.map((id) => document.getElementById(id)?.textContent);
		expect(descriptions).toEqual(["New runs only", "Could not save folder"]);
		setError(undefined);
		expect(input.getAttribute("aria-describedby")!.split(" ")).toHaveLength(1);
		expect(view.getByRole("region", { name: "Workspace" })).toBeTruthy();
	});

	// Catches: neutral unknown status gets presented as a verified success.
	it("retains the caller's unknown status, source and freshness without inference", () => {
		const view = render(() => (
			<StatusSummary label="Status not available" source="Last configuration read" freshness="Not checked" />
		));
		expect(view.getByRole("status").textContent).toContain("Last configuration read · Not checked");
		expect(view.getByText("Status not available").getAttribute("data-variant")).toBe("default");
	});

	// Catches: shared danger presentation executes a mutation or invents confirmation.
	it("leaves destructive action and confirmation ownership with the caller", () => {
		let calls = 0;
		const view = render(() => (
			<DangerZone title="Disconnect" scope="This connection" consequence="Active clients disconnect.">
				<button onClick={() => calls++}>Review disconnect</button>
			</DangerZone>
		));
		expect(calls).toBe(0);
		fireEvent.click(view.getByRole("button", { name: "Review disconnect" }));
		expect(calls).toBe(1);
	});
});
