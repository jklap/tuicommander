import { describe, expect, it, vi } from "vitest";
import {
	getRemoteBaseUrl,
	getRemoteToken,
	previewLogPayload,
	setRemoteBaseUrlLookup,
	setRemoteTokenLookup,
	setTransportLogger,
	transportLogger,
	withRemoteToken,
} from "../transportRuntime";

describe("transport runtime ports", () => {
	it("uses injected logging and remote connection lookup without store imports", () => {
		const debug = vi.fn();
		const warn = vi.fn();
		setTransportLogger({ debug, warn });
		setRemoteBaseUrlLookup((id) => (id === "connected" ? "http://remote.test" : undefined));

		transportLogger().debug("network", "connected");
		expect(debug).toHaveBeenCalledWith("network", "connected");
		expect(getRemoteBaseUrl("connected")).toBe("http://remote.test");
		expect(getRemoteBaseUrl("missing")).toBeUndefined();
	});

	// HTTP, WebSocket and SSE all reach the same daemon; only the query string can
	// carry a credential on all three, so one helper builds it for all three.
	describe("withRemoteToken", () => {
		it("appends the token, choosing the right separator", () => {
			setRemoteTokenLookup((id) => (id === "c1" ? "tok-abc" : undefined));
			expect(getRemoteToken("c1")).toBe("tok-abc");
			expect(withRemoteToken("http://remote.test/api/version", "c1")).toBe(
				"http://remote.test/api/version?token=tok-abc",
			);
			expect(withRemoteToken("ws://remote.test/sessions/s1/stream?format=grid", "c1")).toBe(
				"ws://remote.test/sessions/s1/stream?format=grid&token=tok-abc",
			);
		});

		it("leaves a local call untouched — it has no connection id and needs none", () => {
			setRemoteTokenLookup(() => "tok-abc");
			expect(withRemoteToken("/sessions")).toBe("/sessions");
		});

		// A connection that needs no credential (a desktop instance on the LAN)
		// holds no token; signing its URL with "undefined" would break it.
		it("leaves the URL untouched when the connection holds no token", () => {
			setRemoteTokenLookup(() => undefined);
			expect(withRemoteToken("http://remote.test/api/version", "c1")).toBe("http://remote.test/api/version");
		});
	});

	it("bounds malformed payload previews", () => {
		expect(previewLogPayload("short")).toBe("short");
		expect(previewLogPayload("x".repeat(501))).toBe(`${"x".repeat(500)}...`);
	});
});
