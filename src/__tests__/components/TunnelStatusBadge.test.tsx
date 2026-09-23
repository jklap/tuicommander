import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { TunnelStatusBadge } from "../../components/TunnelsPanel/TunnelStatusBadge";
import type { TunnelStatus } from "../../stores/tunnels";

// Note: this component's color dot is styled with `var(--x, #fallback)` (a
// comma-form CSS custom property fallback). jsdom's CSSStyleDeclaration parser
// rejects that value for `background` and silently drops it from the rendered
// style attribute entirely — even though it's valid CSS and renders correctly
// in a real browser — so the actual dot color can't be asserted from here.
// Colors are presentational and already covered by this repo's own convention
// of a manual screenshot check after any visual change (see AGENTS.md); these
// tests cover the label logic, which is real behavior jsdom CAN observe.

describe("TunnelStatusBadge", () => {
	afterEach(() => cleanup());

	it("shows 'stopped' when no status prop is given", () => {
		const { getByText } = render(() => <TunnelStatusBadge />);
		expect(getByText("stopped")).toBeTruthy();
	});

	it.each([
		{ type: "starting" },
		{ type: "connected" },
		{ type: "stopped", reason: "user requested" },
	] as const satisfies TunnelStatus[])("labels a bare '$type' status by its type", (status) => {
		const { getByText } = render(() => <TunnelStatusBadge status={status} />);
		expect(getByText(status.type)).toBeTruthy();
	});

	it("labels a 'reconnecting' status with its attempt number, not the bare type", () => {
		const { getByText, queryByText } = render(() => (
			<TunnelStatusBadge status={{ type: "reconnecting", attempt: 3, reason: "network blip" }} />
		));
		expect(getByText("reconnecting (#3)")).toBeTruthy();
		expect(queryByText("reconnecting")).toBeNull();
	});

	it("labels an 'error' status by its type, never leaking its message into the badge", () => {
		const { getByText, queryByText } = render(() => (
			<TunnelStatusBadge status={{ type: "error", message: "connection refused" }} />
		));
		expect(getByText("error")).toBeTruthy();
		expect(queryByText("connection refused")).toBeNull();
	});
});
