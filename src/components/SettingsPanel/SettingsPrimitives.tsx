import { createUniqueId, type JSX, Show } from "solid-js";
import { StatusBadge } from "../ui/StatusBadge";
import { SettingField } from "./fields/SettingField";
import { InfoBadge } from "./InfoBadge";
import fieldStyles from "./Settings.module.css";
import s from "./SettingsPrimitives.module.css";

export function SettingPageHeader(props: {
	title: string;
	description: string;
	scope: string;
	source?: JSX.Element;
	actions?: JSX.Element;
	icon?: JSX.Element;
	breadcrumb?: string;
}) {
	return (
		<>
			<Show when={props.breadcrumb}>
				<div class={s.breadcrumb}>{props.breadcrumb}</div>
			</Show>
			<header class={s.pageHeader}>
				<div>
					<h2 class={s.pageTitle}>
						{props.icon}
						{props.title}
					</h2>
					<p>{props.description}</p>
					<Show when={props.source}>
						<p class={s.source}>{props.source}</p>
					</Show>
				</div>
				<div class={s.actions}>
					<StatusBadge label={props.scope} />
					{props.actions}
				</div>
			</header>
		</>
	);
}

export function SectionCard(props: {
	title: string;
	description?: string;
	help?: JSX.Element;
	actions?: JSX.Element;
	children: JSX.Element;
}) {
	const id = createUniqueId();
	return (
		<section class={s.card} aria-labelledby={id}>
			<div class={s.cardHeading}>
				<h3 id={id}>{props.title}</h3>
				{props.help}
			</div>
			<Show when={props.description}>
				<p class={s.description}>{props.description}</p>
			</Show>
			{props.children}
			<Show when={props.actions}>
				<div class={`${s.actions} ${s.cardActions}`}>{props.actions}</div>
			</Show>
		</section>
	);
}

/** SettingField remains the single owner of labels and descriptive IDs. */
export function SettingRow(props: {
	label: string;
	hint?: string;
	error?: string;
	help?: JSX.Element;
	actions?: JSX.Element;
	children: (id: string, descriptionId: () => string | undefined) => JSX.Element;
}) {
	return (
		<SettingField
			label={props.label}
			hint={props.hint}
			error={props.error}
			help={
				props.help && (
					<span class={fieldStyles.settingHelp}>
						<InfoBadge>{props.help}</InfoBadge>
					</span>
				)
			}
			actions={props.actions}
			row
		>
			{(id, _hintId, descriptionId) => props.children(id, descriptionId)}
		</SettingField>
	);
}

export function StatusSummary(props: {
	label: string;
	tone?: "success" | "warning" | "error";
	source: string;
	freshness?: string;
	details?: JSX.Element;
	actions?: JSX.Element;
	children?: JSX.Element;
}) {
	return (
		<div class={s.status} role="status">
			<div>
				<StatusBadge label={props.label} variant={props.tone} />
				<p class={s.source}>
					{props.source}
					<Show when={props.freshness}> · {props.freshness}</Show>
				</p>
				{props.details}
			</div>
			{props.children}
			<Show when={props.actions}>
				<div class={s.actions}>{props.actions}</div>
			</Show>
		</div>
	);
}

export function ListRow(props: { name: string; description?: string; badges?: JSX.Element; actions?: JSX.Element }) {
	return (
		<div class={s.listRow}>
			<div class={s.listText}>
				<span>{props.name}</span>
				<Show when={props.description}>
					<p>{props.description}</p>
				</Show>
				<Show when={props.badges}>
					<div class={s.badges}>{props.badges}</div>
				</Show>
			</div>
			<div class={s.actions}>{props.actions}</div>
		</div>
	);
}

/** The caller chooses whether this represents empty, unsupported or failed state. */
export function EmptyState(props: { title: string; description: string; action?: JSX.Element }) {
	return (
		<div class={s.empty}>
			<strong>{props.title}</strong>
			<p>{props.description}</p>
			{props.action}
		</div>
	);
}

/** Presentation only: callers retain their existing confirmation and mutation. */
export function DangerZone(props: { title: string; scope: string; consequence: string; children: JSX.Element }) {
	const id = createUniqueId();
	return (
		<section class={`${s.card} ${s.danger}`} aria-labelledby={id}>
			<h3 id={id}>{props.title}</h3>
			<p class={s.description}>{props.scope}</p>
			<p>{props.consequence}</p>
			<div class={s.actions}>{props.children}</div>
		</section>
	);
}
