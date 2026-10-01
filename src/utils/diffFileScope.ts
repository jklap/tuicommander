export type DiffFileScope = "lockfile" | "generated" | "test";

const LOCKFILES = new Set([
	"package-lock.json",
	"npm-shrinkwrap.json",
	"pnpm-lock.yaml",
	"yarn.lock",
	"bun.lock",
	"bun.lockb",
	"Cargo.lock",
	"Gemfile.lock",
	"poetry.lock",
	"uv.lock",
	"Pipfile.lock",
	"composer.lock",
	"go.sum",
	"Package.resolved",
	"Podfile.lock",
	"flake.lock",
]);

/** Built-in patterns for files nobody reviews line by line. */
const GENERATED = [
	/\.min\.(js|css)$/,
	/\.map$/,
	/\.snap$/,
	/\.generated\./,
	/\.pb\.go$/,
	/_pb2(_grpc)?\.py$/,
	/\.d\.ts$/,
];
const GENERATED_DIRS = /(^|\/)(dist|build|node_modules|vendor|__snapshots__)\//;

const TESTS = [
	/(^|\/)(__tests__|tests?|spec|e2e)\//,
	/\.(test|spec)\.[a-z0-9]+$/i,
	/_test\.(go|rs|py|rb)$/,
	/(^|\/)test_[^/]+\.py$/,
];

/** Translate a gitattributes pattern to a RegExp. A pattern with no slash matches the
 *  basename at any depth; `*` stops at `/`, `**` crosses it. */
export function globToRegExp(pattern: string): RegExp {
	const anchored = pattern.startsWith("/") || pattern.slice(0, -1).includes("/");
	const body = pattern.replace(/^\//, "").replace(/\/$/, "/**");
	let re = "";
	for (let i = 0; i < body.length; i++) {
		const c = body[i];
		if (c === "*" && body[i + 1] === "*") {
			re += ".*";
			i++;
			if (body[i + 1] === "/") i++;
		} else if (c === "*") re += "[^/]*";
		else if (c === "?") re += "[^/]";
		else re += c.replace(/[.+^${}()|[\]\\]/g, "\\$&");
	}
	return new RegExp(anchored ? `^${re}$` : `(^|/)${re}$`);
}

/** Patterns marked `linguist-generated` (set, or `=true`) in a .gitattributes file. */
export function parseLinguistGenerated(gitattributes: string): string[] {
	const patterns: string[] = [];
	for (const raw of gitattributes.split("\n")) {
		const line = raw.trim();
		if (!line || line.startsWith("#")) continue;
		const [pattern, ...attrs] = line.split(/\s+/);
		if (attrs.some((a) => a === "linguist-generated" || a === "linguist-generated=true")) patterns.push(pattern);
	}
	return patterns;
}

/** Why a file starts collapsed in the PR diff, or null when it should be read. */
export function classifyDiffFile(path: string, generatedPatterns: string[] = []): DiffFileScope | null {
	const base = path.slice(path.lastIndexOf("/") + 1);
	if (LOCKFILES.has(base)) return "lockfile";
	if (GENERATED_DIRS.test(path) || GENERATED.some((r) => r.test(path))) return "generated";
	if (generatedPatterns.some((p) => globToRegExp(p).test(path))) return "generated";
	if (TESTS.some((r) => r.test(path))) return "test";
	return null;
}
