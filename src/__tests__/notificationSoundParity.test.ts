import { describe, expect, it } from "vitest";
import { NOTIFICATION_SOUNDS as CORE_NOTIFICATION_SOUNDS } from "../notifications";
import { NOTIFICATION_SOUNDS as PLUGIN_NOTIFICATION_SOUNDS } from "../plugins/types";

/**
 * `NotificationSound` is defined in three places that must agree: `src/notifications.ts`
 * (the core set, mirrored by the Rust `NotificationSound` enum in
 * `src-tauri/src/notification_sound.rs`) and `src/plugins/types.ts` (the plugin-facing
 * subset/mirror). These have drifted silently before — `plugins/types.ts` was missing
 * "attention" entirely, so a plugin calling `host.playNotificationSound("attention")`
 * silently downgraded to "info" with a spurious "unknown sound" warning, even though
 * "attention" was a fully valid, real sound everywhere else. This test exists so a new
 * sound variant added to one definition but not the other fails loudly instead of
 * shipping a silent capability gap.
 */
describe("NotificationSound enum parity across core and plugin definitions", () => {
	it("plugins/types.ts's NOTIFICATION_SOUNDS matches notifications.ts's exactly", () => {
		expect(new Set(PLUGIN_NOTIFICATION_SOUNDS)).toEqual(new Set(CORE_NOTIFICATION_SOUNDS));
	});
});
