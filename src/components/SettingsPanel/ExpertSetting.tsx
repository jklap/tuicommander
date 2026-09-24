import { type Accessor, type Component, createContext, createSignal, type JSX, onCleanup, Show, useContext } from "solid-js";
import { t } from "../../i18n";
import { settingsExpertStore } from "../../stores/settingsExpert";
import { uiStore } from "../../stores/ui";
import s from "./Settings.module.css";

interface ExpertSectionContext {
	register: (visible: Accessor<boolean>) => void;
}

const SectionContext = createContext<ExpertSectionContext>();

/**
 * A setting the default is right for almost everyone. Hidden in basic mode
 * while `value` equals the default at `configKey`; shown in expert mode, when
 * modified, when a search result revealed it, or while defaults are unknown.
 *
 * `configKey` follows the convention in `stores/settingsExpert.ts` and must be
 * a string literal: the search-index drift test reads it from the source to
 * mark the wrapped labels as expert. `value` is the control's live value in
 * the serialized config shape (snake_case domain, Rust types).
 */
export const ExpertSetting: Component<{ configKey: string; value: unknown; children: JSX.Element }> = (props) => {
	const visible = () => settingsExpertStore.isVisible(props.configKey, props.value);
	useContext(SectionContext)?.register(visible);
	return <Show when={visible()}>{props.children}</Show>;
};

/**
 * A settings section (`<div class={s.section}>`) made only of `ExpertSetting`s.
 * It hides itself, heading included, when every one of them is hidden. Use it
 * only when the section holds no basic control — a basic control does not
 * register, so it would be hidden along with the expert ones.
 */
export const ExpertSection: Component<{ children: JSX.Element }> = (props) => {
	const [members, setMembers] = createSignal<Accessor<boolean>[]>([]);
	const register = (visible: Accessor<boolean>) => {
		setMembers((prev) => [...prev, visible]);
		onCleanup(() => setMembers((prev) => prev.filter((member) => member !== visible)));
	};
	// Kept mounted and hidden, not unmounted: the members must stay registered
	// to bring the section back when one of them becomes visible.
	const hidden = () => {
		const all = members();
		return all.length > 0 && all.every((visible) => !visible());
	};
	return (
		<SectionContext.Provider value={{ register }}>
			<div class={s.section} hidden={hidden()}>
				{props.children}
			</div>
		</SectionContext.Provider>
	);
};

/** The Settings header's "Expert" switch — flips the persisted pref. */
export const ExpertModeSwitch: Component = () => (
	<label class={s.expertSwitch}>
		<span>{t("settings.expert.switch", "Expert")}</span>
		<input
			type="checkbox"
			role="switch"
			checked={uiStore.state.settingsExpertMode}
			onChange={(e) => uiStore.setSettingsExpertMode(e.currentTarget.checked)}
		/>
	</label>
);
