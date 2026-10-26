import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import QRCode from "qrcode";
import { describe, expect, it, vi } from "vitest";

vi.mock("qrcode", () => ({ default: { toDataURL: vi.fn().mockResolvedValue("data:image/png;base64,AA==") } }));

import { SecretForm } from "./SecretForm";

describe("SecretForm", () => {
	it("does not add unrequested fields or retain values while submission is pending", async () => {
		let resolve: (() => void) | undefined;
		const submit = vi.fn(
			() =>
				new Promise<void>((done) => {
					resolve = done;
				}),
		);
		const { container } = render(() => (
			<SecretForm
				form={{
					nonce: "test-nonce",
					id: "test",
					reason: "Login",
					fields: [{ name: "PASS", kind: "password" }],
					argv: null,
				}}
				submit={submit}
			/>
		));
		const input = screen.getByLabelText("PASS");
		if (!(input instanceof HTMLInputElement)) throw new Error("Expected a sensitive input");
		expect(container.querySelectorAll("input")).toHaveLength(1);
		expect(input.type).toBe("password");
		fireEvent.input(input, { target: { value: "synthetic-only" } });
		fireEvent.click(screen.getByRole("button", { name: "Store" }));
		expect(container.querySelectorAll("input")).toHaveLength(0);
		expect(container.textContent).not.toContain("synthetic-only");
		expect(submit).toHaveBeenCalledWith({
			nonce: "test-nonce",
			status: "stored",
			values: { PASS: "synthetic-only" },
			template: null,
		});
		resolve?.();
	});

	it("decline does not forward partially typed values", async () => {
		const submit = vi.fn(async () => {});
		const { container } = render(() => (
			<SecretForm
				form={{ nonce: "test-nonce", id: "test", reason: "Login", fields: [{ name: "OTP", kind: "otp" }], argv: null }}
				submit={submit}
			/>
		));
		fireEvent.input(screen.getByLabelText("OTP"), { target: { value: "123456" } });
		fireEvent.click(screen.getByRole("button", { name: "Decline" }));
		expect(submit).toHaveBeenCalledWith({ nonce: "test-nonce", status: "declined", values: {}, template: null });
		expect(container.querySelectorAll("input")).toHaveLength(0);
		expect(
			screen.getByText("the agent cannot read this value, but a command you approve can send it anywhere"),
		).toBeTruthy();
	});

	it("SSO display does not create an input or silently run the link", () => {
		render(() => (
			<SecretForm
				form={{
					nonce: "test-nonce",
					id: "test",
					reason: "Login",
					fields: [{ name: "SSO", kind: "sso", display: "https://example.invalid/login" }],
					argv: null,
				}}
				submit={async () => {}}
			/>
		));
		expect(screen.queryByRole("textbox")).toBeNull();
		expect(screen.getByText("https://example.invalid/login")).toBeTruthy();
		expect(screen.queryByRole("link")).toBeNull();
	});
	// Catches: the form forwarding an editable/wildcard template instead of
	// retaining only the exact argv that the human sees and approves.
	it("remembers only the displayed exact argv", async () => {
		const submit = vi.fn(async () => {});
		const argv = ["/usr/bin/gh", "api", "user"];
		render(() => (
			<SecretForm
				form={{
					nonce: "test-nonce",
					id: "test",
					reason: "Approve",
					fields: [{ name: "TOKEN", kind: "password" }],
					argv,
					cwd: "/trusted",
				}}
				submit={submit}
			/>
		));
		expect(screen.queryByRole("textbox")).toBeNull();
		fireEvent.click(screen.getByRole("checkbox"));
		fireEvent.click(screen.getByRole("button", { name: "Approve" }));
		expect(submit).toHaveBeenCalledWith({
			nonce: "test-nonce",
			status: "approved",
			values: {},
			template: argv,
		});
	});
});

// Catches: the private form displaying a bare path with no usable phone actions.
it("shares the same absolute nonce URL through link copy and QR", async () => {
	const url = "https://host.tail.example:9876/index.html#/secret-form?nonce=" + "a".repeat(64);
	const writeText = vi.fn().mockResolvedValue(undefined);
	Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
	const view = render(() => (
		<SecretForm
			form={{ id: "phone", nonce: "a".repeat(64), reason: "Login", fields: [], argv: null, mobile_url: url }}
			submit={async () => {}}
		/>
	));
	const link = view.getByRole("link", { name: url, hidden: true });
	expect(link.getAttribute("href")).toBe(url);
	expect(link.getAttribute("referrerpolicy")).toBe("no-referrer");
	fireEvent.click(view.getByRole("button", { name: "Copy form URL", hidden: true }));
	await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(url));
	await waitFor(() =>
		expect(view.getByAltText("QR code for private secret entry").getAttribute("src")).toBe(
			"data:image/png;base64,AA==",
		),
	);
	expect(QRCode.toDataURL).toHaveBeenCalledWith(url, expect.objectContaining({ width: 320 }));
	fireEvent.click(view.getByRole("button", { name: "Decline" }));
	expect(view.queryByRole("link", { hidden: true })).toBeNull();
});

// Catches: disabled remote access still advertising a bare or unusable phone link.
it("explains disabled remote access without exposing a link or QR", () => {
	const reason = "Remote Access is off. Enable it in Settings → Remote Access, then request a new form.";
	const view = render(() => (
		<SecretForm
			form={{ id: "off", nonce: "b".repeat(64), reason: "Login", fields: [], argv: null, mobile_unavailable: reason }}
			submit={async () => {}}
		/>
	));
	expect(view.getByText(reason)).toBeTruthy();
	expect(view.queryByRole("link", { hidden: true })).toBeNull();
	expect(view.queryByRole("button", { name: "Copy form URL", hidden: true })).toBeNull();
	expect(view.queryByAltText("QR code for private secret entry")).toBeNull();
});

// Catches: a native link navigating the private WebView instead of opening the browser.
it("routes native link and copy actions to the private window adapters", async () => {
	const url = "http://[fd00::1]:9876/index.html#/secret-form?nonce=" + "c".repeat(64);
	const openUrl = vi.fn().mockResolvedValue(undefined);
	const copyUrl = vi.fn().mockRejectedValue(new Error("clipboard unavailable"));
	const view = render(() => (
		<SecretForm
			form={{ id: "native", nonce: "c".repeat(64), reason: "Login", fields: [], argv: null, mobile_url: url }}
			submit={async () => {}}
			openUrl={openUrl}
			copyUrl={copyUrl}
		/>
	));
	fireEvent.click(view.getByRole("link", { name: url, hidden: true }));
	expect(openUrl).toHaveBeenCalledExactlyOnceWith(url);
	fireEvent.click(view.getByRole("button", { name: "Copy form URL", hidden: true }));
	await waitFor(() => expect(view.getByText("Copy failed. Select and copy the URL manually.")).toBeTruthy());
	expect(copyUrl).toHaveBeenCalledExactlyOnceWith(url);
});

// Catches: QR renderer failure taking down entry or hiding the usable fallback URL.
it("retains the link and copy action when QR generation fails", async () => {
	vi.mocked(QRCode.toDataURL).mockRejectedValueOnce(new Error("QR unavailable"));
	const url = "https://host.example/index.html#/secret-form?nonce=" + "d".repeat(64);
	const view = render(() => (
		<SecretForm
			form={{ id: "qr-error", nonce: "d".repeat(64), reason: "Login", fields: [], argv: null, mobile_url: url }}
			submit={async () => {}}
		/>
	));
	await waitFor(() => expect(view.getByText("QR unavailable. Open or copy the URL instead.")).toBeTruthy());
	expect(view.getByRole("link", { name: url, hidden: true })).toBeTruthy();
	expect(view.getByRole("button", { name: "Copy form URL", hidden: true })).toBeTruthy();
});
