import { createSignal, For, Show } from "solid-js";
import "./SecretForm.css";

export interface SecretField {
	name: string;
	kind: "username" | "password" | "otp" | "sso";
	display?: string;
}
export interface PrivateForm {
	id: string;
	nonce: string;
	reason: string;
	fields: SecretField[];
	argv: string[] | null;
	mobile_url?: string | null;
	cwd?: string | null;
}
export interface SecretSubmission {
	nonce: string;
	status: "stored" | "approved" | "declined";
	values: Record<string, string>;
	template: string[] | null;
}

export function SecretForm(props: { form: PrivateForm; submit: (submission: SecretSubmission) => Promise<void> }) {
	const [closed, setClosed] = createSignal(false);
	const [message, setMessage] = createSignal("");
	const inputs = new Map<string, HTMLInputElement>();
	let remember: HTMLInputElement | undefined;
	let templateInput: HTMLTextAreaElement | undefined;
	const finish = async (declined: boolean) => {
		const values: Record<string, string> = {};
		let template: string[] | null = null;
		if (!declined && !props.form.argv) {
			for (const [name, input] of inputs) {
				if (!input.value) {
					setMessage("Fill every requested field or decline.");
					return;
				}
				values[name] = input.value;
			}
		}
		if (!declined && props.form.argv && remember?.checked) {
			try {
				const parsed: unknown = JSON.parse(templateInput?.value ?? "null");
				if (!Array.isArray(parsed) || !parsed.every((item) => typeof item === "string")) throw new Error("invalid");
				template = parsed;
			} catch {
				setMessage("The template must be a JSON array of arguments.");
				return;
			}
		}
		// Clear native/browser input properties and unmount BEFORE the IPC/fetch.
		// No reactive store, localStorage, log, event or terminal receives a value.
		for (const input of inputs.values()) input.value = "";
		inputs.clear();
		if (templateInput) templateInput.value = "";
		setClosed(true);
		const submission: SecretSubmission = {
			nonce: props.form.nonce,
			status: declined ? "declined" : props.form.argv ? "approved" : "stored",
			values,
			template,
		};
		try {
			await props.submit(submission);
			setMessage("Complete. You can close this window.");
		} catch {
			setMessage("Submission failed or expired. Close this window and request a new form.");
		} finally {
			for (const name of Object.keys(values)) values[name] = "";
			submission.nonce = "";
		}
	};
	return (
		<section class="private-secret-form">
			<h1>{props.form.argv ? "Approve secret command" : "Private secret entry"}</h1>
			<p>{props.form.reason}</p>
			<p class="secret-limit">the agent cannot read this value, but a command you approve can send it anywhere</p>
			<Show when={!closed()} fallback={<p role="status">{message() || "Submitting…"}</p>}>
				<Show when={props.form.argv}>
					<p>Exact argv:</p>
					<pre>{JSON.stringify(props.form.argv, null, 2)}</pre>
					<p>
						Working directory: <code>{props.form.cwd}</code>
					</p>
					<p>Secret names: {props.form.fields.map((field) => field.name).join(", ")}</p>
					<label class="secret-remember">
						<input type="checkbox" ref={remember} /> Allow this template for these names and directory until exit
					</label>
					<label>
						Argv template (whole <code>{"{arg}"}</code> placeholders for gh api only; other commands use exact argv)
						<textarea ref={templateInput} spellcheck={false} value={JSON.stringify(props.form.argv)} />
					</label>
				</Show>
				<Show when={!props.form.argv}>
					<For each={props.form.fields}>
						{(field) => (
							<Show
								when={field.kind !== "sso"}
								fallback={
									<div class="secret-sso">
										<strong>{field.name}</strong>
										<pre>{field.display}</pre>
										<p>Open this link on your trusted device to complete SSO.</p>
									</div>
								}
							>
								<label>
									{field.name}
									<input
										ref={(input) => inputs.set(field.name, input)}
										name={field.name}
										type="password"
										autocomplete="off"
										spellcheck={false}
										autocapitalize="off"
										inputmode={field.kind === "otp" ? "numeric" : "text"}
									/>
								</label>
							</Show>
						)}
					</For>
				</Show>
				<Show when={props.form.mobile_url}>
					<details>
						<summary>Open on a trusted browser or phone</summary>
						<p>Phone entry requires the HTTPS link. A loopback HTTP link works on this computer only.</p>
						<pre>{props.form.mobile_url}</pre>
					</details>
				</Show>
				<p role="alert">{message()}</p>
				<div class="secret-actions">
					<button
						type="button"
						onClick={() => {
							void finish(true);
						}}
					>
						Decline
					</button>
					<button
						type="button"
						class="secret-primary"
						onClick={() => {
							void finish(false);
						}}
					>
						{props.form.argv ? "Approve" : "Store"}
					</button>
				</div>
			</Show>
		</section>
	);
}
