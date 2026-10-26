import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

interface Appearance {
	name: string;
	focused: boolean;
	outline: string;
	offset: string;
	border: string;
	shadow: string;
	accent: string;
	error: string;
}

// Real Chromium + fixture Vite, like ChatReadability.test.ts. Ordinary Vitest is opt-in.
const url = process.env.TUIC_FOCUS_BROWSER_URL;
describe.skipIf(!url)("browser-rendered shared control focus", () => {
	const samples: Appearance[] = [];
	beforeAll(() => {
		const directory = process.env.TUIC_FOCUS_EVIDENCE_DIR;
		if (!directory) throw new Error("TUIC_FOCUS_EVIDENCE_DIR is required");
		execFileSync("node", [join(process.cwd(), "src/__tests__/browser/captureFormFocus.mjs"), url!, directory], {
			timeout: 240000,
			stdio: ["ignore", "pipe", "pipe"],
		});
		samples.push(...JSON.parse(readFileSync(join(directory, "computed-focus.json"), "utf8")));
	}, 250000);

	const expectTextFocus = (sample: Appearance) => {
		expect.soft(sample.focused, sample.name).toBe(true);
		expect.soft(sample.outline, sample.name).toBe("none");
		expect.soft(sample.offset, sample.name).toBe("0px");
		expect.soft(sample.shadow, sample.name).toContain("inset");
		expect.soft(sample.shadow, sample.name).toContain("0px 0px 0px 1px");
		expect.soft(sample.shadow, sample.name).toContain(sample.accent);
		expect.soft(sample.border, sample.name).toBe(sample.name === "Invalid description" ? sample.error : sample.accent);
	};

	it("hugs mounted Input and Textarea borders — catches offset outlines doubling text-field focus", () => {
		expect(samples.map((sample) => sample.name)).toEqual([
			"Title",
			"Compact title",
			"Description",
			"Invalid description",
			"Action",
			"Choice",
			"Enabled",
			"Volume",
			"Blue",
			"Story title",
			"Story description",
			"Workflow title",
			"Workflow description",
			"Comment popover",
		]);
		for (const sample of [...samples.slice(0, 4), samples[13]]) expectTextFocus(sample);
	});

	it("keeps consumer text focus border-hugging — catches feature CSS restoring detached outlines", () => {
		for (const sample of samples.slice(9, 13)) expectTextFocus(sample);
	});

	it("retains non-text keyboard rings — catches a broad focus reset hiding action focus", () => {
		for (const sample of samples.slice(4, 9)) {
			expect.soft(sample.focused, sample.name).toBe(true);
			expect.soft(sample.outline, sample.name).toBe("solid");
			expect.soft(sample.offset, sample.name).toBe("2px");
		}
	});
});
