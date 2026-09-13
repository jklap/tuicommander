import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toastsStore } from "../../stores/toasts";
import { MobileToastContainer } from "../components/MobileToastContainer";

describe("MobileToastContainer", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	});

	afterEach(() => {
		cleanup();
		vi.runOnlyPendingTimers();
		vi.useRealTimers();
	});

	it("shows the origin repository and dismisses without desktop navigation", () => {
		toastsStore.add("built", "ok", "info", false, undefined, undefined, "/Gits/personal/ego", "remote-session");
		render(() => <MobileToastContainer />);

		expect(screen.getByText("ego")).toBeTruthy();
		fireEvent.click(screen.getByText("built"));
		expect(toastsStore.toasts).toHaveLength(0);
	});

	it("runs an action without also invoking the dismiss surface", () => {
		const action = vi.fn();
		toastsStore.add("Update ready", "", "info", false, { label: "Install", onClick: action });
		render(() => <MobileToastContainer />);

		fireEvent.click(screen.getByText("Install"));
		expect(action).toHaveBeenCalledOnce();
		expect(toastsStore.toasts).toHaveLength(0);
	});
});
