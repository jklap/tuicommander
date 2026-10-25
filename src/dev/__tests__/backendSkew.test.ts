import { afterEach, expect, it, vi } from "vitest";
import { startBackendSkewWarning } from "../backendSkew";

afterEach(() => {
	document.body.replaceChildren();
	vi.unstubAllGlobals();
});

it("does not nag after frontend-only landings and clears a resolved mismatch on HMR", async () => {
	const reply = vi
		.fn()
		.mockResolvedValue(new Response(JSON.stringify({ fingerprint: "same", git_hash: "frontend-commit" })));
	vi.stubGlobal("fetch", reply);
	let update: (() => void) | undefined;
	const stop = startBackendSkewWarning(
		{ fingerprint: "same", git_hash: "backend-commit" },
		{
			on: (_event, callback) => {
				update = callback;
			},
			off: vi.fn(),
		},
	);
	await vi.waitFor(() => expect(reply).toHaveBeenCalledOnce());
	expect(document.querySelector('[role="alert"]')).toBeNull();
	reply.mockResolvedValue(new Response(JSON.stringify({ fingerprint: "changed", git_hash: "backend-change" })));
	update?.();
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
	reply.mockResolvedValue(new Response(JSON.stringify({ fingerprint: "same", git_hash: "reverted" })));
	update?.();
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).toBeNull());
	stop();
});

it("keeps a dismissed mismatch quiet across HMR but warns for a new backend tree", async () => {
	const reply = vi
		.fn()
		.mockImplementation(() =>
			Promise.resolve(new Response(JSON.stringify({ fingerprint: "changed", git_hash: "sha" }))),
		);
	vi.stubGlobal("fetch", reply);
	let update: (() => void) | undefined;
	const stop = startBackendSkewWarning(
		{ fingerprint: "old", git_hash: "sha" },
		{
			on: (_event, callback) => {
				update = callback;
			},
			off: vi.fn(),
		},
	);
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
	(document.querySelector('[aria-label="Dismiss backend restart warning"]') as HTMLButtonElement).click();
	update?.();
	await vi.waitFor(() => expect(reply).toHaveBeenCalledTimes(2));
	expect(document.querySelector('[role="alert"]')).toBeNull();
	reply.mockResolvedValue(new Response(JSON.stringify({ fingerprint: "another", git_hash: "sha" })));
	update?.();
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
	stop();
});

it("warns for a pre-feature backend instead of silently treating missing identity as a match", async () => {
	vi.stubGlobal(
		"fetch",
		vi.fn().mockResolvedValue(new Response(JSON.stringify({ fingerprint: "tree", git_hash: "sha" }))),
	);
	// Real Vite exposes off(); Vitest's default hot stub does not.
	const stop = startBackendSkewWarning(undefined, { on: vi.fn(), off: vi.fn() });
	await vi.waitFor(() =>
		expect(document.querySelector('[role="alert"]')?.textContent).toContain("Backend identity unavailable"),
	);
	stop();
});
