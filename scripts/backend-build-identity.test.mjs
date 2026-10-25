import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import assert from "node:assert/strict";
import { backendBuildIdentity } from "./backend-build-identity.mjs";

test("frontend-only commits do not request a backend restart, but dirty Rust and lockfile changes do", () => {
	const root = mkdtempSync(join(tmpdir(), "backend-identity-"));
	const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "pipe" });
	try {
		mkdirSync(join(root, "src-tauri/src"), { recursive: true });
		writeFileSync(join(root, "src-tauri/Cargo.toml"), "[package]\nname='fixture'\n");
		writeFileSync(join(root, "src-tauri/Cargo.lock"), "version = 4\n");
		writeFileSync(join(root, "src-tauri/src/lib.rs"), "fn command() {}\n");
		writeFileSync(join(root, "frontend.ts"), "old UI\n");
		git("init", "--quiet");
		git("add", ".");
		const commit = () => git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "test: record fixture");
		commit();
		const initial = backendBuildIdentity(root);
		writeFileSync(join(root, "frontend.ts"), "new UI\n");
		git("add", "."); commit();
		const uiOnly = backendBuildIdentity(root);
		assert.notEqual(uiOnly.git_hash, initial.git_hash);
		assert.equal(uiOnly.fingerprint, initial.fingerprint);
		writeFileSync(join(root, "src-tauri/src/lib.rs"), "fn new_command() {}\n");
		const rust = backendBuildIdentity(root);
		assert.equal(rust.git_hash, uiOnly.git_hash);
		assert.notEqual(rust.fingerprint, uiOnly.fingerprint);
		writeFileSync(join(root, "src-tauri/Cargo.lock"), "version = 4\n# changed dependencies\n");
		assert.notEqual(backendBuildIdentity(root).fingerprint, rust.fingerprint);
	} finally { rmSync(root, { recursive: true, force: true }); }
});

// Catches: an extension allowlist ignores embedded extraction JavaScript or
// native compilation inputs while the running backend retains their old bytes.
test("embedded extraction scripts and native build inputs change the backend fingerprint", () => {
	const root = mkdtempSync(join(tmpdir(), "backend-embedded-inputs-"));
	const inputs = [
		"src-tauri/src/design_mode/extract.js",
		"src-tauri/patches/webrtc/src/audio.cc",
		"src-tauri/patches/webrtc/src/wrapper.hpp",
		"src-tauri/patches/webrtc/meson.build",
	];
	const excluded = [
		"src-tauri/src/fixtures/capture.json",
		"src-tauri/docs/backend.md",
		"src-tauri/binaries/tuic-bridge",
	];
	const write = (file, content) => {
		mkdirSync(join(root, file, ".."), { recursive: true });
		writeFileSync(join(root, file), content);
	};
	const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "pipe" });
	try {
		write("src-tauri/Cargo.toml", "[package]\nname='fixture'\n");
		for (const file of [...inputs, ...excluded]) write(file, "original compiled input\n");
		git("init", "--quiet");
		git("add", ".");
		git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "test: record backend fixture");
		let previous = backendBuildIdentity(root).fingerprint;
		for (const file of inputs) {
			write(file, "changed compiled input\n");
			const changed = backendBuildIdentity(root).fingerprint;
			assert.notEqual(changed, previous, `${file} must invalidate the compiled backend identity`);
			previous = changed;
		}
		for (const file of excluded) write(file, "changed non-runtime data\n");
		assert.equal(backendBuildIdentity(root).fingerprint, previous, "excluded non-runtime trees must remain quiet");
	} finally { rmSync(root, { recursive: true, force: true }); }
});
