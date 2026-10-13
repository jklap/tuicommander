import { readFileSync, writeFileSync } from "node:fs";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Fixture for the app-owned config HTTP boundary. The production Rust codec has a round-trip test.
// This is browser UI/reload evidence, not a claim about a running desktop backend.
const prefsPath = process.env.TASKS_PREFS_FIXTURE_PATH;
if (!prefsPath) throw new Error("TASKS_PREFS_FIXTURE_PATH must name an isolated file under Gits");
export default defineConfig({
	define: { __APP_VERSION__: JSON.stringify("1.8.0") },
	plugins: [
		solid(),
		{
			name: "tasks-preferences-fixture",
			configureServer(server) {
				server.middlewares.use("/config/ui-prefs", (request, response) => {
					response.setHeader("Content-Type", "application/json");
					if (request.method === "GET") {
						response.end(readFileSync(prefsPath, "utf8"));
						return;
					}
					let body = "";
					request.on("data", (chunk) => {
						body += chunk;
					});
					request.on("end", () => {
						const { config } = JSON.parse(body);
						writeFileSync(prefsPath, JSON.stringify(config, null, 2));
						response.end("null");
					});
				});
			},
		},
	],
	optimizeDeps: { entries: ["src/components/Tasks/TasksPreferences.preview.html"] },
});
