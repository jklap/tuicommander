import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import {
	ConnectionStatusBadge,
	remoteConnectionStatusColor,
	remoteConnectionStatusLabel,
} from "../../../components/shared/ConnectionStatusBadge";

describe("ConnectionStatusBadge", () => {
	afterEach(() => cleanup());

	it("renders the given label", () => {
		const { getByText } = render(() => <ConnectionStatusBadge color="red" label="connected" />);
		expect(getByText("connected")).toBeTruthy();
	});
});

describe("remoteConnectionStatusColor / remoteConnectionStatusLabel", () => {
	it.each([
		["connected", "Connected", "var(--success)"],
		["connecting", "Connecting...", "var(--activity)"],
		["deploying", "Deploying: preparing", "var(--activity)"],
		["error", "Error", "var(--error)"],
		["disconnected", "Disconnected", "var(--fg-muted)"],
		// Reachable but rejected. Not green (a lie) and not red (the network is
		// fine) — the fix is a password, and the label has to say which.
		["unauthenticated", "Not authenticated", "var(--warning)"],
	] as const)("maps %s status without changing its label or color", (status, label, color) => {
		expect(remoteConnectionStatusLabel(status)).toBe(label);
		expect(remoteConnectionStatusColor(status)).toBe(color);
	});

	it("names the current deploy step", () => {
		expect(remoteConnectionStatusLabel("deploying", "uploading binary")).toBe("Deploying: uploading binary");
	});
});
