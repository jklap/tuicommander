import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

afterEach(() => {
	vi.unstubAllGlobals();
	document.body.innerHTML = "";
	history.replaceState(null, "", "/");
});

// Catches: index.html's browser Tauri shim selecting native bootstrap, or
// form fetches omitting the existing server's authentication cookie.
it("loads and submits the nonce form through the authenticated browser router", async () => {
	const nonce = "a".repeat(64);
	history.replaceState(null, "", `/index.html#/secret-form?nonce=${nonce}`);
	vi.stubGlobal("__TAURI_INTERNALS__", {});
	vi.stubGlobal("__TAURI_SHIM__", true);
	const fetch = vi
		.fn()
		.mockResolvedValueOnce(
			new Response(
				JSON.stringify({
					id: "test",
					nonce,
					reason: "Login",
					argv: null,
					fields: [{ name: "PASS", kind: "password" }],
				}),
				{ status: 200 },
			),
		)
		.mockResolvedValueOnce(new Response("{}", { status: 200 }));
	vi.stubGlobal("fetch", fetch);
	const root = document.createElement("main");
	document.body.appendChild(root);
	const { startSecretForm } = await import("./secretForm");
	await startSecretForm(root);
	expect(fetch).toHaveBeenNthCalledWith(
		1,
		`/secrets/forms/${nonce}`,
		expect.objectContaining({
			credentials: "same-origin",
			cache: "no-store",
		}),
	);
	expect(window.location.hash).toBe("#/secret-form");
	fireEvent.input(screen.getByLabelText("PASS"), { target: { value: "synthetic-only" } });
	fireEvent.click(screen.getByRole("button", { name: "Store" }));
	await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
	expect(fetch).toHaveBeenNthCalledWith(
		2,
		"/secrets/forms/submit",
		expect.objectContaining({
			method: "POST",
			credentials: "same-origin",
			body: JSON.stringify({ nonce, status: "stored", values: { PASS: "synthetic-only" }, template: null }),
		}),
	);
	expect(root.querySelector("input")).toBeNull();
	expect(root.textContent).not.toContain("synthetic-only");
});
