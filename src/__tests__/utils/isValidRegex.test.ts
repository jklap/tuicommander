import { describe, expect, it } from "vitest";
import { isValidRegex } from "../../utils/isValidRegex";

describe("isValidRegex", () => {
	it.each(["", "\\S+", "[a-z]+\\.ts", "(?<path>[^\\s:]+):(\\d+)", "https?://\\S+"])("accepts %j", (pattern) => {
		expect(isValidRegex(pattern)).toBe(true);
	});

	it.each(["(", "[a-z", "*foo", "a{2,1}", "(?<name>x)(?<name>y)", "\\"])("rejects %j", (pattern) => {
		expect(isValidRegex(pattern)).toBe(false);
	});

	// The smart-selection engine compiles with "g"; validity must not depend on it.
	it("agrees with a global-flag compile", () => {
		for (const pattern of ["\\S+", "(", "[a-z"]) {
			let globalValid = true;
			try {
				new RegExp(pattern, "g");
			} catch {
				globalValid = false;
			}
			expect(isValidRegex(pattern)).toBe(globalValid);
		}
	});
});
