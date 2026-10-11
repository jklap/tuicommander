import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn(async () => undefined),
	HttpRpcError: class extends Error {},
}));
vi.mock("../../services/uploadAttachment", () => ({ uploadAttachment: vi.fn() }));

import { uploadAttachment } from "../../services/uploadAttachment";
import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.resetAllMocks();
});

it("does not queue an incomplete prompt while its selected attachment is still uploading", async () => {
	vi.useFakeTimers();
	let finishUpload!: (receipt: { path: string; size: number }) => void;
	vi.mocked(uploadAttachment).mockReturnValue(
		new Promise((resolve) => {
			finishUpload = resolve;
		}),
	);
	vi.mocked(rpc).mockResolvedValue(undefined as never);
	const { container, getByRole } = render(() => <CommandInput sessionId="queue-upload" />);
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "Review this file" } });
	fireEvent.change(container.querySelector('input[type="file"]')!, {
		target: { files: [new File(["important context"], "notes.txt", { type: "text/plain" })] },
	});
	const send = getByRole("button", { name: "Send" });
	fireEvent.pointerDown(send, { button: 0, pointerType: "touch" });
	await vi.advanceTimersByTimeAsync(600);
	fireEvent.pointerUp(send, { button: 0, pointerType: "touch" });
	fireEvent.click(send);
	fireEvent.click(getByRole("menuitem", { name: "Queue" }));
	await Promise.resolve();
	await Promise.resolve();
	// The selected attachment belongs to this prompt, not an unrelated producer.
	// Submitting now can run the agent without the file and erase its later receipt.
	expect(vi.mocked(rpc).mock.calls.filter(([command]) => command === "enqueue_agent_command")).toEqual([]);
	finishUpload({ path: "/repo/.tuic/attachments/notes.txt", size: 17 });
	await Promise.resolve();
	await Promise.resolve();
	expect(input.value).toBe("Review this file @/repo/.tuic/attachments/notes.txt");
});
