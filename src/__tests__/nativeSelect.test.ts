// @vitest-environment node
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import ts from "@typescript/typescript6";

const root = join(import.meta.dirname, "..");
function nativeSelectFiles(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const file = join(directory, entry.name);
		if (entry.isDirectory()) {
			return entry.name === "__tests__" || file === join(root, "components/ui") ? [] : nativeSelectFiles(file);
		}
		if (!/\.tsx$/.test(entry.name) || /\.(test|spec)\.tsx$/.test(entry.name)) return [];
		const source = ts.createSourceFile(file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
        let found = false;
        const visit = (node: ts.Node) => {
            if ((ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) && ts.isIdentifier(node.tagName) && node.tagName.text === "select") found = true;
            ts.forEachChild(node, visit);
        };
        visit(source);
        return found ? [relative(root, file).replaceAll("\\", "/")] : [];
	});
}

describe("shared form control boundary", () => {
	// Catches: a new screen bypassing shared sizing, focus and disabled styling with a native select.
	it("rejects native selects outside ui", () => {
		expect(nativeSelectFiles(root).sort()).toEqual([]);
	});
});
