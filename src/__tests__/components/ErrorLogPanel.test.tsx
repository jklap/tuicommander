import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorLogPanel } from "../../components/ErrorLogPanel/ErrorLogPanel";
import { appLogger } from "../../stores/appLogger";
import { errorLogStore } from "../../stores/errorLog";

/**
 * The panel is mounted for the whole app lifetime — App renders it
 * unconditionally and the `Show` inside gates only the JSX. Solid memos are
 * eager: they re-run whenever a dependency changes, whether or not anything
 * reads them. So a panel nobody has opened was still re-filtering the entire
 * log ring on every single log line, forever.
 *
 * The observable is `appLogger.getEntries` — the memos' one expensive call.
 */
describe("ErrorLogPanel memo gating", () => {
	beforeEach(() => {
		errorLogStore.close();
		appLogger.clear();
		vi.restoreAllMocks();
	});

	it("does not filter the log ring while the panel is closed", () => {
		render(() => <ErrorLogPanel />);
		const getEntries = vi.spyOn(appLogger, "getEntries");

		for (let i = 0; i < 10; i++) appLogger.error("app", `boom ${i}`);

		expect(getEntries).not.toHaveBeenCalled();
	});

	/**
	 * The contrast that proves the test above is not vacuous: once opened, the
	 * same entries must be filtered and rendered.
	 */
	it("filters the log ring once the panel is opened", () => {
		const { container } = render(() => <ErrorLogPanel />);
		appLogger.error("app", "visible failure");

		const getEntries = vi.spyOn(appLogger, "getEntries");
		errorLogStore.open();

		expect(getEntries).toHaveBeenCalled();
		expect(container.textContent).toContain("visible failure");
	});

	it("stops filtering again after the panel is closed", () => {
		render(() => <ErrorLogPanel />);
		errorLogStore.open();
		errorLogStore.close();

		const getEntries = vi.spyOn(appLogger, "getEntries");
		for (let i = 0; i < 5; i++) appLogger.warn("app", `later ${i}`);

		expect(getEntries).not.toHaveBeenCalled();
	});
});
