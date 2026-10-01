import { beforeEach, describe, expect, it, vi } from "vitest";

const showNativeNotice = vi.fn().mockResolvedValue(undefined);
const config = { pr_native_notifications: true };

vi.mock("../../services/nativeNotifications", () => ({ showNativeNotice }));
vi.mock("../../stores/notifications", () => ({ notificationsStore: { state: { config } } }));

const base = { repoName: "acme/api", prNumber: 12, title: "Fix login", url: "https://github.com/acme/api/pull/12" };

describe("notifyPrTransition", () => {
	let notifyPrTransition: typeof import("../../services/prNativeNotifications").notifyPrTransition;

	beforeEach(async () => {
		vi.resetModules();
		showNativeNotice.mockClear();
		config.pr_native_notifications = true;
		notifyPrTransition = (await import("../../services/prNativeNotifications")).notifyPrTransition;
	});

	it.each(["ready", "ci_failed", "changes_requested", "merged"] as const)(
		"sends one notification with repo, number and title for %s",
		(type) => {
			// Catches: transitions reaching only the bell.
			notifyPrTransition({ ...base, type });
			expect(showNativeNotice).toHaveBeenCalledOnce();
			const notice = showNativeNotice.mock.calls[0][0];
			expect(notice.body).toBe("acme/api #12: Fix login");
			expect(notice.target).toEqual({ kind: "pr", url: base.url });
		},
	);

	it("notifies once when the same transition is delivered twice", () => {
		// Catches: duplicate notifications on re-delivery.
		notifyPrTransition({ ...base, type: "ready" });
		notifyPrTransition({ ...base, type: "ready" });
		expect(showNativeNotice).toHaveBeenCalledOnce();
	});

	it("notifies again for a different transition of the same PR", () => {
		notifyPrTransition({ ...base, type: "ci_failed" });
		notifyPrTransition({ ...base, type: "ready" });
		expect(showNativeNotice).toHaveBeenCalledTimes(2);
	});

	it("stays silent for transitions outside the four and when the setting is off", () => {
		notifyPrTransition({ ...base, type: "ci_recovered" });
		notifyPrTransition({ ...base, type: "closed" });
		config.pr_native_notifications = false;
		notifyPrTransition({ ...base, type: "merged" });
		expect(showNativeNotice).not.toHaveBeenCalled();
	});
});
