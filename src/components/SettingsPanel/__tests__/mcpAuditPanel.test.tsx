import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { rpc } = vi.hoisted(() => ({ rpc: vi.fn() }));
vi.mock("../../../transport", () => ({ rpc }));
vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { McpAuditPanel } from "../tabs/services/McpAuditPanel";

const row = (over: Record<string, unknown> = {}) => ({
	call_id: "11111111-1111-4111-8111-111111111111",
	client_call_id: null,
	parent_operation_id: null,
	operation: "session.list",
	target_digest: "abcdef0123456789".repeat(4),
	started_ms: 1_760_000_000_000,
	response_text_bytes: 321,
	application_error: false,
	transport_error: "unobserved_after_reply_construction",
	effect: "completed",
	outcome: "replied",
	...over,
});

const projection = (records: unknown[]) => ({ enabled: true, records, retained: records.length, evicted: 0 });

describe("McpAuditPanel", () => {
	beforeEach(() => rpc.mockReset());
	afterEach(cleanup);

	// Catches: a row rendering response bodies or secrets instead of the body-free whitelist.
	it("renders only the body-free columns even when a row carries extra fields", async () => {
		rpc.mockResolvedValue(
			projection([row({ response_body: "SECRET_BODY", arguments: { token: "SECRET_TOKEN" }, error: "SECRET_ERR" })]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(1));
		expect(container.textContent).not.toContain("SECRET");
		expect(container.textContent).toContain("session.list");
		expect(container.textContent).toContain("321");
		expect(container.textContent).toContain("abcdef01");
		expect(container.textContent).not.toContain("abcdef0123456789abcdef0123456789");
	});

	// Catches: an unknown effect (or a reply) being shown as success.
	it("labels unknown effect as unknown and a cancelled call as cancelled, never as success", async () => {
		rpc.mockResolvedValue(
			projection([
				row({ effect: "unknown", outcome: "cancelled", response_text_bytes: null, application_error: null }),
			]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(1));
		const text = container.querySelector("tbody tr")?.textContent ?? "";
		expect(text).toContain("Cancelled");
		expect(text).toContain("Unknown");
		expect(text).not.toMatch(/success|completed|ok/i);
	});

	// Catches: the outcome filter hiding nothing, or hiding the wrong rows.
	it("filters rows by outcome", async () => {
		rpc.mockResolvedValue(
			projection([
				row({ operation: "repo.list", outcome: "replied" }),
				row({ operation: "progress", outcome: "error", application_error: true }),
			]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(2));
		const select = container.querySelector("select") as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "error" } });
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(1));
		expect(container.querySelector("tbody tr")?.textContent).toContain("progress");
	});

	// Catches: stale rows or no refresh — one fetch on open, one more per Refresh click, none otherwise.
	it("fetches once on open and again only on demand", async () => {
		vi.useFakeTimers();
		try {
			rpc.mockResolvedValue(projection([row()]));
			const { getByText } = render(() => <McpAuditPanel />);
			await vi.advanceTimersByTimeAsync(0);
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("get_mcp_audit");
			await vi.advanceTimersByTimeAsync(120_000);
			expect(rpc).toHaveBeenCalledTimes(1);
			fireEvent.click(getByText("Refresh"));
			await vi.advanceTimersByTimeAsync(0);
			expect(rpc).toHaveBeenCalledTimes(2);
		} finally {
			vi.useRealTimers();
		}
	});

	// Catches: a disabled backend rendering an empty table that looks like "nothing happened".
	it("shows the disabled state, not an empty table, when the backend reports disabled", async () => {
		rpc.mockResolvedValue({ enabled: false, records: [] });
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.textContent).toContain("disabled"));
		expect(container.querySelector("table")).toBeNull();
	});
});
