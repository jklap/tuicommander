/* This entry deliberately does not import App, debugGlobals, appLogger, stores,
 * heartbeat, crash handlers, plugins or the terminal. It has no opener bridge. */

import { invoke } from "@tauri-apps/api/core";
import { render } from "solid-js/web";
import { type PrivateForm, SecretForm, type SecretSubmission } from "./components/SecretForm/SecretForm";
import "./global.css";

const root = document.getElementById("secret-form-root");
if (!root) throw new Error("Private form root is unavailable");
const native = "__TAURI_INTERNALS__" in window;
let nonce = new URLSearchParams(window.location.hash.slice(1)).get("nonce");
// Remove the capability from address/history before sending any request.
if (!native) history.replaceState(null, "", window.location.pathname);

async function start() {
	const form: PrivateForm = native ? await invoke<PrivateForm>("secret_form_bootstrap") : await readForm();
	if (root) render(() => <SecretForm form={form} submit={submit} />, root);
}
async function readForm(): Promise<PrivateForm> {
	if (nonce?.length !== 64) throw new Error("Unknown form");
	const response = await fetch(`/secrets/forms/${encodeURIComponent(nonce)}`, {
		cache: "no-store",
		credentials: "omit",
		referrerPolicy: "no-referrer",
	});
	if (!response.ok) throw new Error("Expired form");
	const value: unknown = await response.json();
	if (!isPrivateForm(value)) throw new Error("Invalid form schema");
	return value;
}
async function submit(submission: SecretSubmission): Promise<void> {
	if (native) await invoke("secret_form_submit", { submission });
	else {
		const response = await fetch("/secrets/forms/submit", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(submission),
			cache: "no-store",
			credentials: "omit",
			referrerPolicy: "no-referrer",
		});
		if (!response.ok) throw new Error("Submission rejected");
	}
	nonce = null;
}
void start().catch(() => {
	if (root) root.textContent = "Private form unavailable or expired. Ask for a new request.";
});

function isPrivateForm(value: unknown): value is PrivateForm {
	if (
		!value ||
		typeof value !== "object" ||
		!("id" in value) ||
		typeof value.id !== "string" ||
		!("nonce" in value) ||
		typeof value.nonce !== "string" ||
		!("reason" in value) ||
		typeof value.reason !== "string" ||
		!("fields" in value) ||
		!Array.isArray(value.fields) ||
		!("argv" in value)
	)
		return false;
	return (
		(value.argv === null || (Array.isArray(value.argv) && value.argv.every((arg) => typeof arg === "string"))) &&
		value.fields.every(
			(field: unknown) =>
				!!field &&
				typeof field === "object" &&
				"name" in field &&
				typeof field.name === "string" &&
				"kind" in field &&
				["username", "password", "otp", "sso"].includes(String(field.kind)),
		)
	);
}
