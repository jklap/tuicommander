import { describe, expect, it } from "vitest";
import { t } from "../../i18n/t";

describe("t", () => {
	it("returns the fallback unchanged when there are no params", () => {
		expect(t("some.key", "Hello world")).toBe("Hello world");
	});

	it("interpolates a param into the fallback", () => {
		expect(t("some.key", "Branch {branch} not found", { branch: "develop" })).toBe("Branch develop not found");
	});

	it("interpolates multiple params", () => {
		expect(t("some.key", "{a} and {b}", { a: "one", b: "two" })).toBe("one and two");
	});

	it("treats a value containing '$&' as literal text, not a replace() special pattern", () => {
		expect(t("some.key", "Branch {branch} not found", { branch: "foo$&bar" })).toBe("Branch foo$&bar not found");
	});

	it("treats a value containing '$1' as literal text", () => {
		expect(t("some.key", "Branch {branch} not found", { branch: "release/$1-fix" })).toBe(
			"Branch release/$1-fix not found",
		);
	});

	it("treats a value containing '$$' as literal text", () => {
		expect(t("some.key", "Branch {branch} not found", { branch: "cost-$$-report" })).toBe(
			"Branch cost-$$-report not found",
		);
	});
});
