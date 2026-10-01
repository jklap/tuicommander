import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("tauri.conf.json assetProtocol scope", () => {
	// Catches: Tauri's Unix default (requireLiteralLeadingDot=true) makes `/**` skip
	// any path with a dot-directory (e.g. ~/Gits/.tmp), so local images there 403.
	it("lets the asset protocol serve files under dot-directories", () => {
		const conf = JSON.parse(readFileSync(resolve(__dirname, "../../src-tauri/tauri.conf.json"), "utf-8"));
		const scope = conf.app.security.assetProtocol.scope;
		expect(scope.requireLiteralLeadingDot).toBe(false);
		expect(scope.allow).toContain("/**");
	});
});
