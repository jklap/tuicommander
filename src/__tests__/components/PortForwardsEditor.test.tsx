import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PortForwardsEditor } from "../../components/TunnelsPanel/PortForwardsEditor";
import type { ForwardSpec } from "../../stores/tunnels";

describe("PortForwardsEditor", () => {
	afterEach(() => cleanup());

	it("+ Add appends a new Local forward defaulting remote_host to defaultRemoteHost", () => {
		const onChange = vi.fn();
		const { getByText } = render(() => (
			<PortForwardsEditor forwards={[]} onChange={onChange} defaultRemoteHost="example.test" />
		));

		fireEvent.click(getByText("+ Add"));

		expect(onChange).toHaveBeenCalledWith([{ type: "Local", bind_port: 0, remote_host: "example.test" }]);
	});

	it("x removes the given row", () => {
		const forwards: ForwardSpec[] = [
			{ type: "Local", bind_port: 8080, remote_host: "h", remote_port: 80 },
			{ type: "Local", bind_port: 9090, remote_host: "h2", remote_port: 90 },
		];
		const onChange = vi.fn();
		const { getAllByText } = render(() => (
			<PortForwardsEditor forwards={forwards} onChange={onChange} defaultRemoteHost="" />
		));

		fireEvent.click(getAllByText("x")[0]);

		expect(onChange).toHaveBeenCalledWith([forwards[1]]);
	});

	it("switching a row's type to Remote converts it via convertForwardType", () => {
		const forwards: ForwardSpec[] = [{ type: "Local", bind_port: 8080, remote_host: "h", remote_port: 80 }];
		const onChange = vi.fn();
		const { container } = render(() => (
			<PortForwardsEditor forwards={forwards} onChange={onChange} defaultRemoteHost="" />
		));

		const select = container.querySelector("select") as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "Remote" } });

		expect(onChange).toHaveBeenCalledWith([
			{ type: "Remote", bind_port: 8080, local_host: "127.0.0.1", local_port: 80 },
		]);
	});
});
