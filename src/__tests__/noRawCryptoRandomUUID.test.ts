import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { readSourceWithoutComments, sourceFiles } from "./helpers/sourceFiles";

/**
 * `crypto.randomUUID` is only defined in a secure context (https, or
 * localhost). TUIC is also reached over plain http on a LAN address, so a
 * bare call throws for exactly the remote clients that need the id most —
 * see `src/utils/randomId.ts`, which exists to paper over this.
 *
 * Ten call sites bypassed that wrapper and called `crypto.randomUUID()`
 * directly, including the browser-mode session-id path in `usePty.ts`. A
 * source scan rather than a per-file test, because the defect is the
 * *existence* of a new raw call site anywhere in the tree, which no single
 * component test can see.
 *
 * Comments are stripped before matching, so a file explaining *why* it
 * doesn't call `crypto.randomUUID()` directly (the same style already used in
 * `randomId.ts`'s own doc comment) can't false-fail this guard.
 */
describe("raw crypto.randomUUID usage", () => {
	const root = join(process.cwd(), "src");
	const allowed = new Set([join(root, "utils", "randomId.ts")]);

	it("only randomId.ts calls crypto.randomUUID directly", () => {
		const offenders = sourceFiles(root)
			.filter((file) => !allowed.has(file))
			.filter((file) => /crypto\.randomUUID\s*\(/.test(readSourceWithoutComments(file)))
			.map((file) => relative(process.cwd(), file));

		expect(offenders).toEqual([]);
	});
});
