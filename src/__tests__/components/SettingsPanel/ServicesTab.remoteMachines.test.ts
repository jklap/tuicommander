import { describe, expect, it } from "vitest";
import {
	emptyRemoteForm,
	remoteStatusColor,
	remoteStatusLabel,
	transportSummary,
} from "../../../components/SettingsPanel/tabs/ServicesTab";

describe("ServicesTab remote machine presentation", () => {
	it.each([
		["connected", "Connected", "var(--accent-green, #22c55e)"],
		["connecting", "Connecting...", "var(--fg-warning, #e5a100)"],
		["error", "Error", "var(--accent-red, #ef4444)"],
		["disconnected", "Disconnected", "var(--fg-muted)"],
	])("maps %s status without changing its label or color", (status, label, color) => {
		expect(remoteStatusLabel(status)).toBe(label);
		expect(remoteStatusColor(status)).toBe(color);
	});

	it("summarizes both supported transports", () => {
		expect(
			transportSummary({
				type: "Ssh",
				ssh: {
					host: "dev.example.test",
					port: 2222,
					user: "boss",
					identity_file: null,
					server_alive_interval: 15,
					server_alive_count_max: 3,
					strict_host_key_checking: "Yes",
				},
				remote_daemon_port: 9877,
			}),
		).toBe("boss@dev.example.test:2222");
		expect(transportSummary({ type: "Direct", url: "https://dev.example.test" })).toBe("https://dev.example.test");
		expect(transportSummary({ type: "Local", port: 9877, instance_id: null })).toBe("local: 127.0.0.1:9877");
		expect(transportSummary({ type: "Local", port: null, instance_id: "dev-box" })).toBe("local: dev-box");
	});

	it("keeps the new-machine defaults stable", () => {
		expect(emptyRemoteForm()).toEqual({
			name: "",
			transportType: "Ssh",
			sshHost: "",
			sshPort: 22,
			sshUser: "",
			identityFile: "",
			sshServerAliveInterval: 15,
			sshServerAliveCountMax: 3,
			sshStrictHostKeyChecking: "Yes",
			// Fixed (plan Phase 1): matches `RemoteConnection::new_ssh`'s Rust
			// default and what a real `tuic-remote` daemon actually listens on.
			remoteDaemonPort: 9877,
			directUrl: "",
			authUsername: "",
		});
	});
});
