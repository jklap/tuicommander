import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";

const source = readFileSync(resolve(__dirname, "../../../public/sw.js"), "utf-8");

type Listener = (event: unknown) => void;

/** Run public/sw.js against stubs and return its `fetch` listener plus the cache spy. */
function loadWorker(response: { ok: boolean; status?: number; type?: string }) {
	const listeners: Record<string, Listener> = {};
	const put = vi.fn();
	const caches = {
		open: async () => ({ put }),
		match: async () => undefined,
		keys: async () => [],
	};
	const self = {
		addEventListener: (name: string, fn: Listener) => {
			listeners[name] = fn;
		},
	};
	const fetchStub = vi.fn(async () => ({ ...response, clone: () => ({ cloned: true }) }));
	new Function("self", "caches", "fetch", "clients", "crypto", source)(
		self,
		caches,
		fetchStub,
		{},
		{ randomUUID: () => "k" },
	);
	return { listeners, put, fetchStub };
}

async function navigate(path: string, response: { ok: boolean; status?: number; type?: string }) {
	const worker = loadWorker(response);
	let pending: Promise<unknown> | undefined;
	worker.listeners.fetch({
		request: { method: "GET", url: `http://tuic.test:9876${path}`, mode: "navigate" },
		respondWith: (p: Promise<unknown>) => {
			pending = p;
		},
	});
	await pending;
	await new Promise((r) => setTimeout(r, 0));
	return worker;
}

describe("service worker and the login flow", () => {
	// Plausible bug: the worker stores the opaque redirect that the server answers an expired session with.
	it("does not cache the opaque redirect to the login page", async () => {
		const { put } = await navigate("/mobile", { ok: false, status: 0, type: "opaqueredirect" });
		expect(put).not.toHaveBeenCalled();
	});

	// Plausible bug: the worker intercepts POST /auth/login and swallows the cookie.
	it("does not intercept the login POST", () => {
		const { listeners } = loadWorker({ ok: true });
		const respondWith = vi.fn();
		listeners.fetch({
			request: { method: "POST", url: "http://tuic.test:9876/auth/login", mode: "cors" },
			respondWith,
		});
		expect(respondWith).not.toHaveBeenCalled();
	});
});
