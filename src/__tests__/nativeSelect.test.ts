// @vitest-environment node
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import allowlist from "./nativeSelectAllowlist.json";

const root = join(import.meta.dirname, "..");
function nativeSelectFiles(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const file = join(directory, entry.name);
		if (entry.isDirectory()) {
			return entry.name === "__tests__" || file === join(root, "components/ui") ? [] : nativeSelectFiles(file);
		}
		if (!/\.tsx$/.test(entry.name) || /\.(test|spec)\.tsx$/.test(entry.name)) return [];
		return /<select\b/.test(readFileSync(file, "utf8")) ? [relative(root, file).replaceAll("\\", "/")] : [];
	});
}

describe("shared form control boundary", () => {
	// Catches: a new screen bypassing shared sizing, focus and disabled styling with a native select.
	it("rejects native selects outside ui except explicitly pending migrations", () => {
		expect(nativeSelectFiles(root).sort()).toEqual([...allowlist].sort());
	});
});
