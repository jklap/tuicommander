import { readFileSync } from "node:fs";
import { gzipSync } from "node:zlib";

const CHECK = process.argv.includes("--check");

const entries = [
	{ page: "dist/index.html", maxGzipBytes: 500 * 1024 },
	// Bumped 100KB -> 115KB (2026-09-25): AgentWrapPromptHost joined the
	// "mounted by both shells" host-component family (McpConfirmHost,
	// PtyOpenUrlHost) that mobile must load eagerly (it can't be lazy — no
	// backend replay-on-reconnect for a missed one-shot prompt), pushing
	// mobile.html to ~108KB after trimming what could safely be deferred
	// (agentConfigsStore's full store moved to a lazy import on the
	// answer-only path). This family will keep growing by a few KB per
	// future parity feature; ~10KB of headroom over the current ~108KB,
	// not a blank check — if a future addition eats through it, look for
	// the same "store/module only needed on a rare write-path but
	// statically imported" shape before bumping this again.
	{ page: "dist/mobile.html", maxGzipBytes: 115 * 1024 },
];

const deferredAssetPattern =
	/(?:CodeEditorTab|createCodeMirror|DiffFileList|ContentRenderer|MarkdownTab|ComposePanel|PrDiffTab|katex|cytoscape|mermaid)/i;

let failed = false;

for (const entry of entries) {
	const html = readFileSync(entry.page, "utf8");
	const assets = [
		...new Set(
			[...html.matchAll(/(?:src|href)="\/(assets\/[^\"]+\.(?:js|css))"/g)].map((match) => match[1]),
		),
	];

	let rawBytes = 0;
	let gzipBytes = 0;
	for (const asset of assets) {
		const content = readFileSync(`dist/${asset}`);
		rawBytes += content.length;
		gzipBytes += gzipSync(content).length;
	}

	console.log(`${entry.page}: ${assets.length} assets, ${rawBytes} bytes raw, ${gzipBytes} bytes gzip`);

	if (!CHECK) continue;

	const eagerDeferredAssets = assets.filter((asset) => deferredAssetPattern.test(asset));
	if (eagerDeferredAssets.length > 0) {
		failed = true;
		console.error(`${entry.page}: optional assets returned to the initial load graph:`);
		for (const asset of eagerDeferredAssets) console.error(`  - ${asset}`);
	}

	if (gzipBytes > entry.maxGzipBytes) {
		failed = true;
		console.error(`${entry.page}: ${gzipBytes} gzip bytes exceeds the ${entry.maxGzipBytes}-byte budget`);
	}
}

if (failed) process.exitCode = 1;
