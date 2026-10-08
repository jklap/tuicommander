import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";

const { store } = vi.hoisted(() => ({
	store: {
		pending: (): unknown => null,
		resolveProvisionConfirmation: vi.fn(),
	},
}));

vi.mock("../../../stores/remoteConnections", () => ({
	remoteConnectionsStore: {
		getPendingProvisionConfirmation: () => store.pending(),
		resolveProvisionConfirmation: (accepted: boolean) => store.resolveProvisionConfirmation(accepted),
	},
}));

import { ProvisionConfirmDialog } from "../../../components/shared/ProvisionConfirmDialog";

const launch =
	"read -r T; cd ~/.cache/tuic && if [ -f tuic-remote.pid ]; then P=$(cat tuic-remote.pid 2>/dev/null); fi; TUIC_PAIRING_TOKEN=$T ./tuic-remote";

const plan = {
	connection_id: "c1",
	connection_name: "Build box",
	action: "start",
	destination: "boss@box.example.test:22",
	summary: "Start tuic-remote v1.7.7 on boss@box.example.test:22 (127.0.0.1:9877).",
	steps: [
		{ description: "Detect the remote platform", command: "uname -sm" },
		{ description: "Upload the pinned asset if it differs" },
		{ description: "Start it", command: launch },
	],
	digest: "d".repeat(64),
};

describe("ProvisionConfirmDialog", () => {
	afterEach(() => {
		cleanup();
		store.resolveProvisionConfirmation.mockClear();
	});

	it("renders nothing while no plan is waiting", () => {
		store.pending = () => null;
		const { container } = render(() => <ProvisionConfirmDialog />);
		expect(container.textContent).toBe("");
	});

	it("shows the destination and every remote command verbatim", () => {
		const [pending] = createSignal<unknown>(plan);
		store.pending = pending;
		const { getAllByTestId, getByText } = render(() => <ProvisionConfirmDialog />);
		expect(getByText("boss@box.example.test:22")).toBeTruthy();
		expect(getAllByTestId("provision-command").map((el) => el.textContent)).toEqual(["uname -sm", launch]);
		expect(getByText("Upload the pinned asset if it differs")).toBeTruthy();
		expect(store.resolveProvisionConfirmation).not.toHaveBeenCalled();
	});

	it("runs only on an explicit accept; cancel and the backdrop decline", () => {
		const [pending] = createSignal<unknown>(plan);
		store.pending = pending;
		const { getByText, getByRole } = render(() => <ProvisionConfirmDialog />);

		fireEvent.click(getByText("Cancel"));
		expect(store.resolveProvisionConfirmation).toHaveBeenLastCalledWith(false);
		fireEvent.click(getByRole("dialog").parentElement as HTMLElement);
		expect(store.resolveProvisionConfirmation).toHaveBeenLastCalledWith(false);
		fireEvent.click(getByText("Accept and run"));
		expect(store.resolveProvisionConfirmation).toHaveBeenLastCalledWith(true);
	});
});
