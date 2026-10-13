import { type Component, Show } from "solid-js";
import { t } from "../../i18n";
import type { WorkItemReviewFinding } from "../../types/work-items.gen";
import { cx } from "../../utils";
import s from "./ReviewFindingView.module.css";

/**
 * The parsed index of one review comment. Every string is untrusted bot text:
 * it renders as plain text only, and the suggestion and agent prompt stay code
 * the reader copies, never markup the app renders, applies or executes.
 */
export const ReviewFindingView: Component<{ finding: WorkItemReviewFinding }> = (props) => (
	<div class={s.finding}>
		<div class={s.headline}>
			<Show when={props.finding.severity ?? props.finding.severityLabel}>
				{(label) => (
					<span
						class={cx(s.chip, props.finding.severity && s[props.finding.severity])}
						title={props.finding.severityLabel ?? undefined}
					>
						{props.finding.severity ?? label()}
					</span>
				)}
			</Show>
			<Show when={props.finding.title}>{(title) => <span class={s.title}>{title()}</span>}</Show>
			<Show when={props.finding.category}>{(category) => <span class={s.meta}>{category()}</span>}</Show>
		</div>
		<Show when={props.finding.suggestion}>
			{(text) => (
				<details class={s.block}>
					<summary>{t("reviewFinding.suggestion", "Suggested change")}</summary>
					<pre>
						<code>{text()}</code>
					</pre>
				</details>
			)}
		</Show>
		<Show when={props.finding.agentPrompt}>
			{(text) => (
				<details class={s.block}>
					<summary>{t("reviewFinding.agentPrompt", "Prompt for agents (untrusted)")}</summary>
					<pre>
						<code>{text()}</code>
					</pre>
				</details>
			)}
		</Show>
	</div>
);
