import { describe, expect, it } from "vitest";
import { transportBadgeLabel, transportSummary } from "../../../components/SettingsPanel/tabs/RemoteServersTab";

describe("RemoteServersTab transport presentation", () => {
	it("summarizes all three remote transports", () => {
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

	it("badges each transport kind", () => {
		expect(
			transportBadgeLabel({
				type: "Ssh",
				ssh: {
					host: "h",
					port: 22,
					user: "u",
					identity_file: null,
					server_alive_interval: 15,
					server_alive_count_max: 3,
					strict_host_key_checking: "Yes",
				},
				remote_daemon_port: 9877,
			}),
		).toBe("SSH");
		expect(transportBadgeLabel({ type: "Direct", url: "http://h" })).toBe("DIRECT");
		expect(transportBadgeLabel({ type: "Local", port: 9877, instance_id: null })).toBe("LOCAL");
	});
});
