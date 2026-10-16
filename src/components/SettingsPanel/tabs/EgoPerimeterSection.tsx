import { type Component, createSignal, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { type EgoPerimeterClient, egoPerimeterCli } from "../../../services/egoCli";
import { appLogger } from "../../../stores/appLogger";
import { asEgoCliError, type EgoPerimeterView, type EgoRootAccess, type EgoRootsEdit } from "../../../types/ego";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Textarea } from "../../ui/Textarea";
import s from "../Settings.module.css";
import { SectionCard, SettingRow } from "../SettingsPrimitives";
import layout from "./AiChatTab.module.css";

export const EgoPerimeterSection: Component<{ client?: EgoPerimeterClient }> = (props) => {
	const client = () => props.client ?? egoPerimeterCli;
	const [view, setView] = createSignal<EgoPerimeterView | null>(null);
	const [draft, setDraft] = createSignal<EgoRootsEdit>({
		rootDir: "",
		rootAccess: "read-write",
		readAllowlist: "",
		writableDirs: "",
	});
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const attempt = async (call: () => Promise<EgoPerimeterView>, adoptRoots: boolean): Promise<void> => {
		setBusy(true);
		try {
			const result = await call();
			if (adoptRoots) setDraft(result.roots);
			setView(result);
			setError(null);
		} catch (failure) {
			const egoError = asEgoCliError(failure);
			if (!egoError) appLogger.error("config", "ego perimeter request failed", failure);
			setError(
				egoError ? `${egoError.message}\n${egoError.command}\n${egoError.stderr || egoError.stdout}` : String(failure),
			);
		} finally {
			setBusy(false);
		}
	};
	onMount(() => void attempt(() => client().perimeter(), true));
	const rootsDirty = () => {
		const saved = view()?.roots;
		return !!saved && JSON.stringify(saved) !== JSON.stringify(draft());
	};
	const edit = <K extends keyof EgoRootsEdit>(key: K, value: EgoRootsEdit[K]) =>
		setDraft((current) => ({ ...current, [key]: value }));

	return (
		<div class={layout.perimeter}>
			<p class={s.hint}>
				{t(
					"perimeter.hint",
					"ego roots extend the AI Chat workspace; they do not replace it. These defaults apply to new ego runs. Running conversations keep their admitted perimeter.",
				)}
			</p>
			<Show when={error()}>
				{(message) => (
					<>
						<p class={s.warning} role="alert">
							{t(
								"perimeter.failure",
								"The write or readback failed. Reload from ego to check what is stored. Check the ego executable and AI Chat workspace in General if the command could not start.",
							)}
						</p>
						<pre class={s.mcpSnippetPre}>{message()}</pre>
					</>
				)}
			</Show>
			<Show when={view()}>
				{(loaded) => (
					<>
						<SectionCard title="Workspace access">
							<p class={s.hint}>
								{t("perimeter.target", "Editing ego user configuration:")}{" "}
								{loaded().profile ?? t("perimeter.userDefaults", "user defaults")}
							</p>
							<SettingRow label={t("perimeter.label.rootDir", "Root directory")}>
								{(id, descriptionId) => (
									<>
										<Input
											type="text"
											id={id}
											aria-describedby={descriptionId()}
											value={draft().rootDir}
											disabled={busy()}
											onInput={(event) => edit("rootDir", event.currentTarget.value)}
											placeholder="~/Gits"
										/>
										<p class={s.hint}>
											{t(
												"perimeter.emptyRoot",
												"Leave all paths empty for workspace-only roots. An absent roots setting defaults to ~/Gits when that directory exists.",
											)}
										</p>
									</>
								)}
							</SettingRow>
							<SettingRow label={t("perimeter.label.rootAccess", "Root access")}>
								{(id, descriptionId) => (
									<>
										<Select
											id={id}
											aria-describedby={descriptionId()}
											value={draft().rootAccess}
											disabled={busy()}
											onChange={(event) => {
												const value = event.currentTarget.value;
												if (value === "read" || value === "read-write")
													edit("rootAccess", value satisfies EgoRootAccess);
											}}
										>
											<option value="read-write">{t("perimeter.readWrite", "Read and write")}</option>
											<option value="read">{t("perimeter.read", "Read only")}</option>
										</Select>
									</>
								)}
							</SettingRow>
							<SettingRow label={t("perimeter.label.readAllowlist", "Read allowlist")}>
								{(id, descriptionId) => (
									<>
										<Textarea
											id={id}
											aria-describedby={descriptionId()}
											rows={2}
											value={draft().readAllowlist}
											disabled={busy()}
											onInput={(event) => edit("readAllowlist", event.currentTarget.value)}
										/>
									</>
								)}
							</SettingRow>
							<SettingRow label={t("perimeter.label.writableDirs", "Extra writable directories")}>
								{(id, descriptionId) => (
									<>
										<Textarea
											id={id}
											aria-describedby={descriptionId()}
											rows={2}
											value={draft().writableDirs}
											disabled={busy()}
											onInput={(event) => edit("writableDirs", event.currentTarget.value)}
										/>
										<p class={s.hint}>
											{t(
												"perimeter.paths",
												"One absolute path or ~/ path per line. Read-only entries do not revoke access granted by a containing writable root. ego validates roots and system ceilings.",
											)}
										</p>
									</>
								)}
							</SettingRow>
							<div class={s.actions}>
								<button
									disabled={busy()}
									data-ready={rootsDirty() ? "true" : undefined}
									onClick={() => void attempt(() => client().setRoots(draft()), true)}
								>
									{t("perimeter.saveRoots", "Save roots in ego")}
								</button>
							</div>
						</SectionCard>
						<SectionCard title="Network and sandbox">
							<label class={s.toggle}>
								<input
									type="checkbox"
									checked={loaded().networkEnabled}
									disabled={busy()}
									onChange={(event) => {
										const enabled = event.currentTarget.checked;
										event.currentTarget.checked = loaded().networkEnabled;
										void attempt(() => client().setNetwork(enabled), false);
									}}
								/>
								<span>{t("perimeter.label.network", "Network enabled")}</span>
							</label>
							<p class={s.hint}>
								{t("perimeter.execSandbox", "Exec sandbox:")}{" "}
								<span class={loaded().execEnforcement === "enforcedByOs" ? s.availabilityOk : s.availabilityBad}>
									{loaded().execEnforcement === "enforcedByOs"
										? t("perimeter.osEnforced", "Enforced by OS")
										: loaded().execEnforcement === "promptOnly"
											? t("perimeter.promptOnly", "Prompt only")
											: t("perimeter.unchecked", "Enforcement not checked")}
								</span>
							</p>
							<Show when={loaded().execEnforcement === "promptOnly"}>
								<p class={s.hint}>
									{t(
										"perimeter.incomplete",
										"The selected exec perimeter is not fully OS-enforced. This badge covers spawned processes; profile-root tool permissions are separate.",
									)}
								</p>
							</Show>
							<Show when={loaded().effective.capabilities_reason}>{(reason) => <p class={s.hint}>{reason()}</p>}</Show>
						</SectionCard>
						<SectionCard title="Advanced and diagnostics">
							<details>
								<summary>{t("perimeter.preview", "Last successful effective perimeter from ego")}</summary>
								<pre class={s.mcpSnippetPre}>{loaded().preview}</pre>
							</details>
						</SectionCard>
					</>
				)}
			</Show>
			<div class={s.actions}>
				<button disabled={busy()} onClick={() => void attempt(() => client().perimeter(), true)}>
					{t("perimeter.reload", "Reload from ego")}
				</button>
			</div>
		</div>
	);
};
