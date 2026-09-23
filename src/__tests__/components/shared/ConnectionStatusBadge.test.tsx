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
		["connected", "Connected", "var(--accent-green, #22c55e)"],
		["connecting", "Connecting...", "var(--fg-warning, #e5a100)"],
		["error", "Error", "var(--accent-red, #ef4444)"],
		["disconnected", "Disconnected", "var(--fg-muted)"],
	] as const)("maps %s status without changing its label or color", (status, label, color) => {
		expect(remoteConnectionStatusLabel(status)).toBe(label);
		expect(remoteConnectionStatusColor(status)).toBe(color);
	});
});
