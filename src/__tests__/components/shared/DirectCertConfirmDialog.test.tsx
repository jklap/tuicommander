import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";

const { store } = vi.hoisted(() => ({
	store: {
		pending: (): unknown => null,
		resolveFingerprintConfirmation: vi.fn(),
	},
}));

vi.mock("../../../stores/remoteConnections", () => ({
	remoteConnectionsStore: {
		getPendingFingerprintConfirmation: () => store.pending(),
		resolveFingerprintConfirmation: (accepted: boolean) => store.resolveFingerprintConfirmation(accepted),
	},
}));

import { DirectCertConfirmDialog } from "../../../components/shared/DirectCertConfirmDialog";

const request = {
	connectionId: "c1",
	connectionName: "Self-signed box",
	url: "https://box:9877",
	fingerprint: "ab".repeat(32),
};

describe("DirectCertConfirmDialog", () => {
	afterEach(() => {
		cleanup();
		store.resolveFingerprintConfirmation.mockClear();
	});

	it("renders nothing while no Connect is waiting", () => {
		store.pending = () => null;
		const { container } = render(() => <DirectCertConfirmDialog />);
		expect(container.textContent).toBe("");
	});

	it("shows the full fingerprint and answers accept or cancel", () => {
		const [pending] = createSignal<unknown>(request);
		store.pending = pending;
		const { getByText, getByTestId } = render(() => <DirectCertConfirmDialog />);
		expect(getByTestId("direct-cert-fingerprint").textContent).toBe(request.fingerprint);

		fireEvent.click(getByText("Cancel"));
		expect(store.resolveFingerprintConfirmation).toHaveBeenLastCalledWith(false);
		fireEvent.click(getByText("Accept and connect"));
		expect(store.resolveFingerprintConfirmation).toHaveBeenLastCalledWith(true);
	});
});
