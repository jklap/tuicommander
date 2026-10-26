import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
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
		mkdirSync(join(directory, "ab-empty"), { recursive: true });
		const wrapper = join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
		const run = (...args: string[]) =>
			execFileSync("perl", ["-e", "alarm 40; exec @ARGV", wrapper, ...args], {
				env: { ...process.env, AB_SESSION: "tuic2377", AB_AGENT_BROWSER_DIR: join(directory, "ab-empty") },
				encoding: "utf8",
				timeout: 45000,
				stdio: ["ignore", "pipe", "pipe"],
			});
		const sample = () =>
			JSON.parse(
				run(
					"--",
					"eval",
					`(() => {
			const node = document.activeElement;
			const ring = node.getAttribute('role') === 'switch' ? node.parentElement : node;
			const style = getComputedStyle(ring);
			const color = (token) => {
				const probe = document.createElement('span');
				probe.style.color = 'var(' + token + ')';
				document.body.append(probe);
				const value = getComputedStyle(probe).color;
				probe.remove();
				return value;
			};
			return { name: node.getAttribute('aria-label') || node.textContent,
				focused: node.matches(':focus-visible'), outline: style.outlineStyle,
				offset: style.outlineOffset, border: style.borderTopColor, shadow: style.boxShadow,
				accent: color('--accent'), error: color('--error') };
		})()`,
				),
			) as Appearance;
		try {
			run("mac-chrome", `${url}/src/__tests__/browser/formFocus.html`);
			run("--", "set", "viewport", "1400", "1100");
			run("--", "wait", "input[aria-label='Title']");
			for (let index = 0; index < 9; index++) {
				run("--", "press", "Tab");
				samples.push(sample());
			}
			run("--", "click", ".tweak-highlight");
			run("--", "wait", "[data-tweak-popover] textarea");
			samples.push({ ...sample(), name: "Comment popover" });
			run("--", "screenshot", "[data-tweak-popover]", join(directory, "comment-popover-1400.png"));
			writeFileSync(join(directory, "computed-focus.json"), JSON.stringify(samples, null, 2));
		} finally {
			run("--", "tab", "close");
		}
	}, 240000);

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
			"Comment popover",
		]);
		for (const sample of [...samples.slice(0, 4), samples[9]]) {
			expect.soft(sample.focused, sample.name).toBe(true);
			expect.soft(sample.outline, sample.name).toBe("none");
			expect.soft(sample.offset, sample.name).toBe("0px");
			expect.soft(sample.shadow, sample.name).toContain("inset");
			expect.soft(sample.shadow, sample.name).toContain("0px 0px 0px 1px");
			expect.soft(sample.shadow, sample.name).toContain(sample.accent);
			expect
				.soft(sample.border, sample.name)
				.toBe(sample.name === "Invalid description" ? sample.error : sample.accent);
		}
	});

	it("retains non-text keyboard rings — catches a broad focus reset hiding action focus", () => {
		for (const sample of samples.slice(4, 9)) {
			expect.soft(sample.focused, sample.name).toBe(true);
			expect.soft(sample.outline, sample.name).toBe("solid");
			expect.soft(sample.offset, sample.name).toBe("2px");
		}
	});
});
