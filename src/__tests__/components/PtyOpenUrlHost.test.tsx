import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const handlers: Record<string, (payload: unknown) => void> = {};

vi.mock("../../transport", () => ({
	subscribeEvents: vi.fn(async (h: Record<string, (payload: unknown) => void>) => {
		Object.assign(handlers, h);
		return () => {};
	}),
}));

vi.mock("../../utils/openUrl", () => ({
	handleOpenUrl: vi.fn(),
}));

import { PtyOpenUrlHost } from "../../components/PtyOpenUrlHost/PtyOpenUrlHost";
import { handleOpenUrl } from "../../utils/openUrl";

const mockHandleOpenUrl = vi.mocked(handleOpenUrl);

afterEach(cleanup);

describe("PtyOpenUrlHost", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders nothing", () => {
		const { container } = render(() => <PtyOpenUrlHost />);
		expect(container.textContent).toBe("");
	});

	it("opens the URL when the backend confirms it", async () => {
		render(() => <PtyOpenUrlHost />);
		// The subscribe() call is async; let it settle before the event fires.
		await Promise.resolve();
		await Promise.resolve();
		handlers["pty-open-url"]({ session_id: "s1", url: "https://example.com" });
		expect(mockHandleOpenUrl).toHaveBeenCalledWith("https://example.com");
	});
});
