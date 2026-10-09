import { describe, expect, it } from "vitest";

const sources = import.meta.glob<string>(
	["../**/*.ts", "../**/*.tsx", "!../**/__tests__/**", "!../**/*.test.*", "!../transport*.ts"],
	{ query: "?raw", import: "default", eager: true },
);

function matches(pattern: RegExp): string[] {
	return Object.entries(sources).flatMap(([file, source]) =>
		Array.from(source.matchAll(pattern), (match) => `${file}: ${match[0]}`),
	);
}

describe("ES2024 native API conventions", () => {
	it("prevents reintroducing global literal regex replacements", () => {
		expect(matches(/\.replace\(\/(?:[^\\/.*+?^$()[\]{}|]|\\[\\/nrt])+\/g\s*,/g)).toEqual([]);
	});
});
