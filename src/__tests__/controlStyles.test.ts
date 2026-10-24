import { describe, expect, it } from "vitest";
// @ts-expect-error The source audit is a Node-only build script.
import { auditControls, auditCss, controlClasses } from "../../scripts/audit-control-styles.mjs";
import exceptions from "./controlStyleExceptions.json";

interface Finding {
	file: string;
	line: number;
	selector: string;
	signature: string;
}

// DEFERRED (2026-10-24) — existing geometry stays exact-match grandfathered,
// one migration story per module or a documented intentional specialized control;
// never allow a whole file.
describe("shared control geometry guard", () => {
	it("catches new or changed feature control sizing and forces removed exceptions to shrink", () => {
		const actual: Finding[] = auditControls();
		const remaining = actual.map((row) => `${row.file}\0${row.signature}`);
		for (const entry of exceptions) {
			if ("reason" in entry) {
				expect(entry.reason).toMatch(/\S/);
				expect(entry).not.toHaveProperty("story");
			} else {
				expect(entry.story).toMatch(/^\d+-[\da-f]{4}$/);
			}
			for (const signature of entry.rules) {
				const key = `${entry.file}\0${signature}`;
				const index = remaining.indexOf(key);
				expect(
					index,
					`Remove stale exception for ${entry.file} (${"story" in entry ? entry.story : entry.reason}): ${signature}`,
				).toBeGreaterThanOrEqual(0);
				if (index >= 0) remaining.splice(index, 1);
			}
		}
		expect(remaining, "Use src/components/ui primitives; do not add feature control geometry").toEqual([]);
	});

	it("catches raw controls in media queries, grouped selectors and class-based JSX", () => {
		const files = ["src/Feature.tsx"];
		const classes = controlClasses(
			files,
			() => `import s from './Feature.module.css';
			const Feature = () => <><button class={s.action}/><Input class={s.field}/></>;`,
		);
		const css = `/* button { padding: 99px } */
			@media (max-width: 600px) { .row button, .row select { min-height: 44px; } }
			.action { padding: 8px; } .field { border: 1px solid red; }
			:is(button, select) { height: 22px; }
			.row input { padding-inline: 4px; } .row textarea { block-size: 80px; }
			.row { padding: 10px; } .row select { width: 100%; }
			input:checked + .slider { border: 0; }`;
		const findings: Finding[] = auditCss("src/Feature.module.css", css, classes.get("src/Feature.module.css"));
		expect(findings.map((row) => row.selector)).toEqual([
			".row button, .row select",
			".action",
			".field",
			":is(button, select)",
			".row input",
			".row textarea",
		]);
	});

	it("catches new declarations even when the selector already has an exception", () => {
		const old = auditCss("src/Feature.module.css", ".row button { padding: 4px; }")[0];
		const changed = auditCss("src/Feature.module.css", ".row button { padding: 4px; height: 20px; }")[0];
		expect(changed.signature).not.toBe(old.signature);
	});
});

it("catches feature CSS resizing a class passed to SegmentedControl undetected", () => {
	const classes = controlClasses(
		["src/Feature.tsx"],
		() => `import s from './Feature.module.css';
			const Feature = () => <SegmentedControl class={s.viewSwitch}/>;`,
	);
	const findings: Finding[] = auditCss(
		"src/Feature.module.css",
		".viewSwitch { height: 20px; padding: 0; border: 0; }",
		classes.get("src/Feature.module.css"),
	);
	expect(findings.map((row) => row.selector)).toEqual([".viewSwitch"]);
});

it("catches ordinary actions hidden beside explicitly marked layout buttons", () => {
	const findings: Finding[] = auditCss(
		"src/Feature.module.css",
		`
		button.item[data-control-layout="row"] { padding: 8px; }
		button.card[data-control-layout="card"]:hover { border-color: red; }
		button[data-control-layout="row"], button.action { height: 20px; }
		button[data-control-layout] { padding: 2px; }
		input[data-control-layout="row"] { height: 18px; }
		button[data-control-layout="action"] { padding: 3px; }`,
	);
	expect(findings.map((row) => row.selector)).toEqual([
		'button[data-control-layout="row"], button.action',
		"button[data-control-layout]",
		'input[data-control-layout="row"]',
		'button[data-control-layout="action"]',
	]);
});

it("catches feature resizing passed to the shared Button", () => {
	const classes = controlClasses(
		["src/Feature.tsx"],
		() => `import s from './Feature.module.css'; const Feature = () => <Button class={s.action}/>;`,
	);
	expect(
		auditCss("src/Feature.module.css", ".action { height: 18px; }", classes.get("src/Feature.module.css")),
	).toHaveLength(1);
});
