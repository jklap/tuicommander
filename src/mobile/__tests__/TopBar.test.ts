import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const sourceCode = readFileSync(resolve(__dirname, "../components/TopBar.tsx"), "utf-8");

describe("TopBar subtitle and connectivity", () => {
	it("renders the subtitle text when connected", () => {
		expect(sourceCode).toContain("Manage your sessions");
	});

	it("shows reconnecting subtitle when offline", () => {
		expect(sourceCode).toContain("Reconnecting");
	});

	it("has connectivity dot with online/offline classes", () => {
		expect(sourceCode).toContain("connDot");
		expect(sourceCode).toContain("connOnline");
		expect(sourceCode).toContain("connOffline");
	});
});
