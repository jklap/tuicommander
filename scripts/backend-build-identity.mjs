import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Shared by the debug Rust build and Vite: HEAD alone also changes for UI-only landings. */
export function backendBuildIdentity(cwd = root) {
	const files = execFileSync("git", ["ls-files", "-z", "--", "src-tauri", "scripts/backend-build-identity.mjs"], { cwd })
		.toString().split("\0").filter((file) =>
			file === "scripts/backend-build-identity.mjs" ||
			(!/(?:^|\/)(?:fixtures|tests|benches|binaries|icons|gen|history)\//.test(file) &&
				/\.(?:rs|m|mm|c|h|cpp|toml|lock|json)$/.test(file)))
		.sort();
	const hash = createHash("sha256");
	const inputs = [];
	for (const file of files) {
		try {
			if (!statSync(join(cwd, file)).isFile()) continue;
			const bytes = readFileSync(join(cwd, file));
			hash.update(`${file}\0${bytes.length}\0`);
			hash.update(bytes);
			inputs.push(file);
		} catch (error) {
			// A tracked deletion is a different tree, not an unreadable build.
			if (error.code !== "ENOENT") throw error;
		}
	}
	if (!inputs.some((file) => file === "src-tauri/Cargo.toml")) throw new Error("Backend build inputs are unavailable");
	return {
		fingerprint: hash.digest("hex"),
		git_hash: execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd }).toString().trim(),
		inputs,
	};
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	process.stdout.write(JSON.stringify(backendBuildIdentity()));
}
