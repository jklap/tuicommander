import { createResource, createSignal, For, Show } from "solid-js";
import { qrCodeDataUrl } from "../../utils/qrCode";
import { Button } from "../ui/Button";
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
	mobile_unavailable?: string | null;
	cwd?: string | null;
}
export interface SecretSubmission {
	nonce: string;
	status: "stored" | "approved" | "declined";
	values: Record<string, string>;
	template: string[] | null;
}

export function SecretForm(props: {
	form: PrivateForm;
	submit: (submission: SecretSubmission) => Promise<void>;
	copyUrl?: (url: string) => Promise<void>;
	openUrl?: (url: string) => Promise<void>;
}) {
	const [closed, setClosed] = createSignal(false);
	const [message, setMessage] = createSignal("");
	const [copyStatus, setCopyStatus] = createSignal("");
	const [qr] = createResource(() => !closed() && props.form.mobile_url, qrCodeDataUrl);
	const copyUrl = async () => {
		const url = props.form.mobile_url;
		if (!url) return;
		try {
			await (props.copyUrl ? props.copyUrl(url) : navigator.clipboard.writeText(url));
			setCopyStatus("Copied");
		} catch {
			setCopyStatus("Copy failed. Select and copy the URL manually.");
		}
	};
	const inputs = new Map<string, HTMLInputElement>();
	let remember: HTMLInputElement | undefined;
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
		if (!declined && props.form.argv && remember?.checked) template = props.form.argv;
		// Clear native/browser input properties and unmount BEFORE the IPC/fetch.
		// No reactive store, localStorage, log, event or terminal receives a value.
		for (const input of inputs.values()) input.value = "";
		inputs.clear();
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
						<input type="checkbox" ref={remember} /> Allow this exact argv for these names and directory until exit
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
				<Show when={props.form.mobile_url || props.form.mobile_unavailable}>
					<details class="secret-phone">
						<summary>Open on a trusted browser or phone</summary>
						<Show when={props.form.mobile_url} fallback={<p>{props.form.mobile_unavailable}</p>}>
							<p>Scan or open this one-time link on a trusted device. Use HTTPS to protect entry in transit.</p>
							<a
								class="secret-phone-url"
								href={props.form.mobile_url!}
								target="_blank"
								rel="noopener noreferrer"
								referrerpolicy="no-referrer"
								onClick={(event) => {
									if (!props.openUrl) return;
									event.preventDefault();
									void props
										.openUrl(props.form.mobile_url!)
										.catch(() => setCopyStatus("Could not open the browser. Copy the URL instead."));
								}}
							>
								{props.form.mobile_url}
							</a>
							<Button aria-label="Copy form URL" onClick={() => void copyUrl()}>
								Copy URL
							</Button>
							<p role="status">{copyStatus()}</p>
							<Show
								when={!qr.error && qr()}
								fallback={<p>{qr.error ? "QR unavailable. Open or copy the URL instead." : "Generating QR…"}</p>}
							>
								<img
									class="secret-phone-qr"
									src={qr()!}
									alt="QR code for private secret entry"
									width={320}
									height={320}
								/>
							</Show>
						</Show>
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
