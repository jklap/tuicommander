import { beforeEach, describe, expect, it, vi } from "vitest";

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

import { subscribePtyOpenUrl } from "../../stores/ptyOpenUrl";
import { subscribeEvents } from "../../transport";
import { handleOpenUrl } from "../../utils/openUrl";

const mockHandleOpenUrl = vi.mocked(handleOpenUrl);

describe("ptyOpenUrl store", () => {
	beforeEach(async () => {
		vi.clearAllMocks();
		await subscribePtyOpenUrl();
	});

	it("subscribes to the pty-open-url event", () => {
		const calls = vi.mocked(subscribeEvents).mock.calls;
		const types = calls[calls.length - 1][0];
		expect(Object.keys(types)).toEqual(["pty-open-url"]);
	});

	it("opens the URL once the backend confirms it", () => {
		handlers["pty-open-url"]({ session_id: "s1", url: "https://example.com" });
		expect(mockHandleOpenUrl).toHaveBeenCalledWith("https://example.com");
	});

	it("passes the URL through verbatim — handleOpenUrl owns the scheme allowlist", () => {
		// This store's only job is wiring the event to handleOpenUrl; the actual
		// scheme validation (http/https/mailto only) is handleOpenUrl's own
		// responsibility and is covered by openUrl.test.ts.
		handlers["pty-open-url"]({ session_id: "s1", url: "javascript:alert(1)" });
		expect(mockHandleOpenUrl).toHaveBeenCalledWith("javascript:alert(1)");
	});
});
