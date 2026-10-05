import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
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
