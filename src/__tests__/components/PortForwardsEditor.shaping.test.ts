import { render } from "@solidjs/testing-library";
import { createComponent } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import {
	convertForwardType,
	normalizeForwardForType,
	PortForwardsEditor,
} from "../../components/TunnelsPanel/PortForwardsEditor";
import type { ForwardSpec } from "../../stores/tunnels";

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(null) }));

describe("PortForwardsEditor forward shaping", () => {
	it("normalizes Remote forwards to the backend local_host/local_port shape", () => {
		const staleRemoteForward: ForwardSpec = {
			type: "Remote",
			bind_port: 9090,
			remote_host: "127.0.0.1",
			remote_port: 3000,
		};

		expect(normalizeForwardForType(staleRemoteForward)).toEqual({
			type: "Remote",
			bind_port: 9090,
			local_host: "127.0.0.1",
			local_port: 3000,
		});
	});

	it("converts Local forwards to Remote without carrying remote-only field names", () => {
		const localForward: ForwardSpec = {
			type: "Local",
			bind_port: 5432,
			remote_host: "db.internal",
			remote_port: 5432,
		};

		expect(convertForwardType(localForward, "Remote")).toEqual({
			type: "Remote",
			bind_port: 5432,
			local_host: "127.0.0.1",
			local_port: 5432,
		});
	});

	it("converts Remote forwards to Local with the current SSH host as the default target", () => {
		const remoteForward: ForwardSpec = {
			type: "Remote",
			bind_port: 8080,
			local_host: "127.0.0.1",
			local_port: 8080,
		};

		expect(convertForwardType(remoteForward, "Local", "example.com")).toEqual({
			type: "Local",
			bind_port: 8080,
			remote_host: "example.com",
			remote_port: 8080,
		});
	});
});

describe("PortForwardsEditor forward row", () => {
	// Catches: forward-row fields left at the 36px default beside the 22px "x" remove button.
	// (main's TunnelEditorModal test; the row is PortForwardsEditor's, which the merged
	// Remote Servers editor renders now that the standalone modal is gone.)
	it("sizes the type select and every port/host field compact, like the remove button", () => {
		const forwards: ForwardSpec[] = [{ type: "Local", bind_port: 5432, remote_host: "db.internal", remote_port: 5432 }];
		const { container } = render(() =>
			createComponent(PortForwardsEditor, { forwards, onChange: () => {}, defaultRemoteHost: "bastion" }),
		);
		const row = container.querySelector('input[placeholder="bind"]')?.parentElement as HTMLElement;
		const fields = [...row.querySelectorAll("select, input")];
		expect(fields).toHaveLength(4);
		for (const field of fields) expect(field.className).toMatch(/compact/);
	});
});
