import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PromptInspector, type PromptReceipt } from "../../components/PromptInspector/PromptInspector";
import { invoke } from "../../invoke";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
const receipt: PromptReceipt = {
	sections: [
		{ label: "Launch brief", source: "stored generator", bytes: 42, text: "é🦀", status: "sent", truncated: false },
		{
			label: "Agent-loaded instructions",
			source: "not observable by TUIC",
			bytes: null,
			text: "Not observable by TUIC",
			status: "not_observable",
			truncated: false,
		},
	],
	captureLimited: false,
};

describe("PromptInspector", () => {
	beforeEach(() => vi.clearAllMocks());
	// Catches: computing counts from rendered/redacted text or hiding stored provenance.
	it("renders stored source and original byte count in collapsible sections", async () => {
		vi.mocked(invoke).mockResolvedValue(receipt);
		const close = vi.fn();
		const view = render(() => <PromptInspector sessionId="launch-1" onClose={close} />);
		await waitFor(() => expect(view.getByText("42 bytes")).toBeTruthy());
		expect(invoke).toHaveBeenCalledWith("get_prompt_receipt", { sessionId: "launch-1" });
		expect(view.container.querySelectorAll("details")).toHaveLength(2);
		expect(view.getByText("stored generator")).toBeTruthy();
		expect(view.getByText("Size unknown")).toBeTruthy();
		expect(view.getByText("é🦀")).toBeTruthy();
		fireEvent.click(view.getByText("Close"));
		expect(close).toHaveBeenCalledOnce();
	});
	// Catches: a previous session's launch text shown while the new request is pending/failed.
	it("hides the previous session receipt on switch and shows a failed read", async () => {
		vi.mocked(invoke).mockResolvedValueOnce(receipt).mockRejectedValueOnce(new Error("Session not found"));
		const [id, setId] = createSignal<string | null>("launch-1");
		const view = render(() => <PromptInspector sessionId={id()} onClose={() => setId(null)} />);
		await waitFor(() => expect(view.getByText("42 bytes")).toBeTruthy());
		setId("launch-2");
		await waitFor(() => expect(view.getByRole("alert")).toBeTruthy());
		expect(view.queryByText("é🦀")).toBeNull();
		setId(null);
		expect(view.queryByRole("dialog")).toBeNull();
	});
	// Catches: presenting queued/capped content as the complete delivered prompt.
	it("labels queued and truncated previews and the capture limit", async () => {
		vi.mocked(invoke).mockResolvedValue({
			sections: [{ ...receipt.sections[0], status: "queued", truncated: true }],
			captureLimited: true,
		});
		const view = render(() => <PromptInspector sessionId="queued" onClose={() => {}} />);
		await waitFor(() => expect(view.getByText("Queued — not yet sent")).toBeTruthy());
		expect(view.getByText(/Truncated redacted preview/)).toBeTruthy();
		expect(view.getByText(/Additional sections omitted/)).toBeTruthy();
	});
});
