import { describe, expect, it } from "vitest";
import { accessDeniedMessage, isAccessDeniedError } from "../../utils/fileReadErrors";

describe("fileReadErrors", () => {
	describe("isAccessDeniedError", () => {
		it("matches the real 403 access-denied RPC message", () => {
			const msg =
				'RPC read_external_file failed: 403 {"error":"Access denied: path must be within a registered repository or an allowed directory"}';
			expect(isAccessDeniedError(msg)).toBe(true);
		});

		it("does not match a generic OS permission-denied error", () => {
			expect(isAccessDeniedError("permission denied (os error 13)")).toBe(false);
		});

		it("does not match a 403 without the access-denied wording", () => {
			expect(isAccessDeniedError("RPC failed: 403 Forbidden")).toBe(false);
		});

		it("does not match access-denied wording without a 403", () => {
			expect(isAccessDeniedError("Access denied: some other reason")).toBe(false);
		});
	});

	describe("accessDeniedMessage", () => {
		it("returns a non-empty, user-facing sentence", () => {
			expect(accessDeniedMessage().length).toBeGreaterThan(0);
		});
	});
});
