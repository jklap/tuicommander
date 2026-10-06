import { type Component, createEffect, createResource, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import s from "./PromptInspector.module.css";

export interface PromptReceipt {
	sections: {
		label: string;
		source: string;
		bytes: number | null;
		text: string;
		status: string;
		truncated: boolean;
	}[];
	captureLimited: boolean;
}

export const PromptInspector: Component<{ sessionId: string | null; onClose: () => void }> = (props) => {
	const [receipt] = createResource(
		() => props.sessionId || undefined,
		(sessionId) => invoke<PromptReceipt>("get_prompt_receipt", { sessionId }),
	);
	createEffect(() => {
		if (props.sessionId) registerModal(props.onClose);
	});
	return (
		<Show when={props.sessionId}>
			<div class={d.overlay} onClick={props.onClose}>
				<section
					class={`${d.popover} ${s.dialog}`}
					role="dialog"
					aria-modal="true"
					aria-label="Prompt inspector"
					onClick={(e) => e.stopPropagation()}
				>
					<div class={d.header}>
						<h4>Prompt inspector</h4>
					</div>
					<div class={d.body}>
						<p class={s.note}>
							Launch-time observations from this session, never current settings. Bytes measure the original UTF-8
							payload before redaction. Sent and served mean TUIC supplied the payload; model consumption is unverified.
						</p>
						<Show when={receipt.loading}>
							<p>Loading launch receipt…</p>
						</Show>
						<Show when={receipt.error}>
							<p role="alert">Could not read the launch receipt. The session may have ended.</p>
						</Show>
						<Show when={!receipt.loading && !receipt.error && receipt()}>
							{(data) => (
								<>
									<For each={data().sections}>
										{(section) => (
											<details class={s.section}>
												<summary>
													{section.label}
													<span class={s.size}>
														{section.bytes === null ? "Size unknown" : `${section.bytes} bytes`}
													</span>
												</summary>
												<p class={s.source}>{section.source}</p>
												<p class={s.note}>
													{section.status === "not_observable"
														? "Not observable by TUIC"
														: section.status === "file_snapshot"
															? "File snapshot supplied at launch; the agent’s actual file read is unverified"
															: section.status === "queued"
																? "Queued — not yet sent"
																: section.status === "served"
																	? "Served in MCP initialize"
																	: "Sent at launch"}
												</p>
												<Show when={section.truncated}>
													<p class={s.note}>Truncated redacted preview (32 KiB per section, 64 KiB per receipt).</p>
												</Show>
												<pre class={s.text}>{section.text}</pre>
											</details>
										)}
									</For>
									<Show when={data().captureLimited}>
										<p class={s.note}>Additional sections omitted by the 16-section capture limit.</p>
									</Show>
								</>
							)}
						</Show>
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={props.onClose}>
							Close
						</button>
					</div>
				</section>
			</div>
		</Show>
	);
};
