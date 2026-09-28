import { describe, expect, it } from "vitest";
import { HttpRpcError } from "../transport";

describe("HTTP RPC error detail", () => {
	// Catches: a JSON error envelope is exposed as raw HTTP metadata in the UI.
	it("extracts the server's text error", () => {
		const error = new HttpRpcError("create_session", 500, '{"error":"Working directory does not exist"}');
		expect(error.detail).toBe("Working directory does not exist");
	});

	// Catches: malformed or non-string error payloads are rendered as empty or object text.
	it.each(["plain failure", "null", '{"error":true}', '{"other":"permission denied"}'])(
		"keeps the response body when it has no string error: %s",
		(body) => {
			expect(new HttpRpcError("list_directory", 500, body).detail).toBe(body);
		},
	);
});
