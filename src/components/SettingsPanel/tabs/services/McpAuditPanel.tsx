import { type Component, createMemo, createSignal, For, onMount, Show } from "solid-js";
import { locale, t } from "../../../../i18n";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { Select } from "../../../ui/Select";
import { type BadgeVariant, StatusBadge } from "../../../ui/StatusBadge";
import s from "../../Settings.module.css";
import css from "./McpAuditPanel.module.css";

/** Body-free row of `GET /mcp/audit`; only these fields are ever read. */
interface AuditRow {
	call_id: string;
	operation: string;
	target_digest: string | null;
	started_ms: number;
	response_text_bytes: number | null;
	effect: string;
	outcome: string;
}

interface AuditSnapshot {
	enabled: boolean;
	records: AuditRow[];
	retained?: number;
}

const OUTCOMES: Record<string, { label: () => string; variant?: BadgeVariant }> = {
	replied: { label: () => t("mcpAudit.outcome.replied", "Replied") },
	error: { label: () => t("mcpAudit.outcome.error", "Error"), variant: "error" },
	cancelled: { label: () => t("mcpAudit.outcome.cancelled", "Cancelled"), variant: "warning" },
};

// "Unknown" is never styled as success: the server did not observe the effect.
const EFFECTS: Record<string, { label: () => string; variant?: BadgeVariant }> = {
	completed: { label: () => t("mcpAudit.effect.completed", "Completed"), variant: "success" },
	not_started: { label: () => t("mcpAudit.effect.notStarted", "Not started") },
	unknown: { label: () => t("mcpAudit.effect.unknown", "Unknown"), variant: "warning" },
};

const DIGEST_CHARS = 8;

/** MCP call audit: newest tools/call observations; fetched on open and on demand only. */
export const McpAuditPanel: Component = () => {
	const [snapshot, setSnapshot] = createSignal<AuditSnapshot | null>(null);
	const [failed, setFailed] = createSignal(false);
	const [filter, setFilter] = createSignal("all");

	const refresh = async () => {
		try {
			setSnapshot(await rpc<AuditSnapshot>("get_mcp_audit"));
			setFailed(false);
		} catch (error) {
			appLogger.warn("config", "Failed to load MCP call audit", error);
			setFailed(true);
		}
	};
	onMount(refresh);

	const rows = createMemo(() =>
		(snapshot()?.records ?? []).filter((r) => filter() === "all" || r.outcome === filter()),
	);

	return (
		<div>
			<div class={css.toolbar}>
				<Select
					compact
					data-non-setting="Filters the audit rows on screen; not a preference"
					aria-label={t("mcpAudit.filter", "Filter by outcome")}
					value={filter()}
					onChange={(e) => setFilter(e.currentTarget.value)}
				>
					<option value="all">{t("mcpAudit.filter.all", "All outcomes")}</option>
					<For each={Object.entries(OUTCOMES)}>{([value, o]) => <option value={value}>{o.label()}</option>}</For>
				</Select>
				<span class={css.spacer} />
				<button
					type="button"
					class={s.testBtn}
					data-non-setting="Reload the audit rows; not a preference"
					onClick={refresh}
				>
					{t("mcpAudit.refresh", "Refresh")}
				</button>
			</div>
			<Show when={failed()}>
				<p class={css.empty} role="alert">
					{t("mcpAudit.failed", "Could not load the call audit.")}
				</p>
			</Show>
			<Show
				when={snapshot()?.enabled !== false}
				fallback={<p class={css.empty}>{t("mcpAudit.disabled", "The call audit is disabled.")}</p>}
			>
				<Show when={rows().length} fallback={<p class={css.empty}>{t("mcpAudit.empty", "No calls recorded yet.")}</p>}>
					<div class={css.tableWrap}>
						<table class={css.table}>
							<colgroup>
								<col class={css.colTime} />
								<col class={css.colOperation} />
								<col class={css.colTarget} />
								<col class={css.colOutcome} />
								<col class={css.colBytes} />
								<col class={css.colEffect} />
							</colgroup>
							<thead>
								<tr>
									<th>{t("mcpAudit.col.time", "Time")}</th>
									<th>{t("mcpAudit.col.operation", "Operation")}</th>
									<th>{t("mcpAudit.col.target", "Target")}</th>
									<th>{t("mcpAudit.col.outcome", "Outcome")}</th>
									<th class={css.num}>{t("mcpAudit.col.bytes", "Bytes")}</th>
									<th>{t("mcpAudit.col.effect", "Effect")}</th>
								</tr>
							</thead>
							<tbody>
								<For each={rows()}>
									{(row) => {
										const outcome = () => OUTCOMES[row.outcome];
										const effect = () => EFFECTS[row.effect] ?? EFFECTS.unknown;
										return (
											<tr>
												<td class={css.mono}>
													{new Date(row.started_ms).toLocaleTimeString(locale(), { hour12: false })}
												</td>
												<td class={`${css.mono} ${css.clip}`} title={row.operation}>
													{row.operation}
												</td>
												<td
													class={`${css.mono} ${css.clip}`}
													title={t("mcpAudit.targetTitle", "SHA-256 digest of the target; not the target")}
												>
													{row.target_digest ? row.target_digest.slice(0, DIGEST_CHARS) : "—"}
												</td>
												<td>
													<StatusBadge label={outcome()?.label() ?? row.outcome} variant={outcome()?.variant} />
												</td>
												<td class={`${css.mono} ${css.num}`}>{row.response_text_bytes ?? "—"}</td>
												<td>
													<StatusBadge label={effect().label()} variant={effect().variant} />
												</td>
											</tr>
										);
									}}
								</For>
							</tbody>
						</table>
					</div>
				</Show>
			</Show>
		</div>
	);
};
