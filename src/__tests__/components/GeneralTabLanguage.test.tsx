import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, mockListen, locales } = vi.hoisted(() => ({
	mockInvoke: vi.fn(),
	mockListen: vi.fn().mockResolvedValue(vi.fn()),
	/** Only `en.json` ships today, so the real list would hide the picker. The
	 * picker tests offer a second locale with no catalog; `t()` stays real. */
	locales: ["en", "it"],
}));

vi.mock("../../i18n", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../i18n")>()),
	AVAILABLE_LOCALES: locales,
}));

vi.mock("../../invoke", () => ({
	invoke: mockInvoke,
	listen: mockListen,
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { GeneralTab } from "../../components/SettingsPanel/tabs/GeneralTab";
import { AVAILABLE_LOCALES, locale, localeName, setLocale, t } from "../../i18n";
import { settingsStore } from "../../stores/settings";

/** The picker, found through its rendered label so it cannot be confused with
 * the other selects the tab renders (IDE, update channel). */
function languageSelect(container: HTMLElement): HTMLSelectElement {
	const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === "Language");
	const select = label?.parentElement?.querySelector("select");
	if (!select) throw new Error("language select not found");
	return select as HTMLSelectElement;
}

function egoProfileInput(container: HTMLElement): HTMLInputElement {
	const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === "ego profile");
	const input = label?.parentElement?.querySelector("input");
	if (!input) throw new Error("ego profile input not found");
	return input;
}

/** A string the picker must retranslate.
 *
 * Why this exists at all: `en.json` is generated from the call sites, so every
 * catalog value equals its inline fallback byte-for-byte. Switching the locale
 * therefore cannot change what an ordinary `t()` site renders — the two sides
 * `t()` chooses between hold the same text — and a test that picked a real UI
 * string could only ever assert that nothing moved. The probe breaks that tie
 * by pairing a key that IS in `en.json` with a fallback that is deliberately
 * not the English text, so the rendered value names which side `t()` resolved
 * from: the fallback means "no catalog for this locale", "Shell" means "read
 * from `en.json`". That is what makes the switch observable in the DOM.
 *
 * The mismatch is safe: `i18nKeyCollisions.test.ts` skips `src/__tests__`, so
 * this fallback is never read as a UI string competing with the real one. */
const Probe = () => <span data-testid="probe">{t("general.label.shell", "UNTRANSLATED FALLBACK")}</span>;

/** Resolve every command the tab's onMount may issue so nothing rejects. */
function invokeImpl(config: Record<string, unknown> = {}) {
	return (cmd: string) => {
		if (cmd === "load_config") return Promise.resolve({ ...config });
		return Promise.resolve(undefined);
	};
}

describe("GeneralTab language picker", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockImplementation(invokeImpl());
		mockListen.mockResolvedValue(vi.fn());
	});

	afterEach(() => {
		cleanup();
		locales.splice(0, locales.length, "en", "it");
		setLocale("en");
		vi.useRealTimers();
	});

	it("hides the picker while only one locale is available", () => {
		locales.splice(0, locales.length, "en");
		const { container } = render(() => <GeneralTab />);
		const labels = Array.from(container.querySelectorAll("label")).map((el) => el.textContent);
		expect(labels).not.toContain("Language");
		// Non-vacuity: the tab rendered, so the absence above is meaningful.
		const headings = Array.from(container.querySelectorAll("h3")).map((el) => el.textContent);
		expect(headings).toContain("Confirmations");
	});

	it("offers one option per locale that has a catalog", () => {
		const { container } = render(() => <GeneralTab />);
		const values = Array.from(languageSelect(container).options).map((o) => o.value);
		expect(values).toEqual([...AVAILABLE_LOCALES]);
		expect(values).toContain("en");
	});

	it("names each locale in its own language", () => {
		const { container } = render(() => <GeneralTab />);
		const labels = Array.from(languageSelect(container).options).map((o) => o.textContent);
		expect(labels).toEqual(AVAILABLE_LOCALES.map((code) => localeName(code)));
		expect(labels).toContain("English");
	});

	it("shows the locale the config was loaded with", async () => {
		mockInvoke.mockImplementation(invokeImpl({ language: "en" }));
		await settingsStore.hydrate();
		const { container } = render(() => <GeneralTab />);
		expect(languageSelect(container).value).toBe("en");
	});

	it("retranslates a rendered string when a locale is picked", async () => {
		// Start on a locale with no catalog, so every t() renders its fallback.
		// This is a real state, not a contrivance: config.json keeps whatever
		// language it was last saved with, including one whose catalog is gone.
		mockInvoke.mockImplementation(invokeImpl({ language: "zz" }));
		await settingsStore.hydrate();

		const { container, getByTestId } = render(() => (
			<>
				<GeneralTab />
				<Probe />
			</>
		));
		expect(locale()).toBe("zz");
		expect(getByTestId("probe").textContent).toBe("UNTRANSLATED FALLBACK");

		fireEvent.change(languageSelect(container), { target: { value: "en" } });

		await waitFor(() => expect(getByTestId("probe").textContent).toBe("Shell"));
		expect(locale()).toBe("en");
	});

	it("persists the picked locale through the config save", async () => {
		vi.useFakeTimers();
		mockInvoke.mockImplementation(invokeImpl({ language: "zz" }));
		await settingsStore.hydrate();

		const { container } = render(() => <GeneralTab />);
		mockInvoke.mockClear();
		fireEvent.change(languageSelect(container), { target: { value: "en" } });

		// The store debounces its writes, so nothing is saved until the timer runs.
		expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "save_config")).toEqual([]);
		await vi.advanceTimersByTimeAsync(600);

		const saved = mockInvoke.mock.calls.filter(([cmd]) => cmd === "save_config");
		expect(saved).toHaveLength(1);
		expect((saved[0][1] as { config: { language: string } }).config.language).toBe("en");
	});
});

describe("GeneralTab ego profile", () => {
	beforeEach(() => {
		mockInvoke.mockImplementation(invokeImpl());
	});

	afterEach(() => {
		settingsStore._testCancelPendingSave();
		cleanup();
	});

	it("keeps the selected profile visible after rejecting a leading dash", () => {
		settingsStore.setEgoProfile("coordinator");
		const { container } = render(() => <GeneralTab />);
		const input = egoProfileInput(container);
		fireEvent.input(input, { target: { value: "-other" } });
		expect(input.value).toBe("coordinator");
		expect(settingsStore.state.egoProfile).toBe("coordinator");
	});
});

function aiChatWorkspaceInput(container: HTMLElement): HTMLInputElement {
	const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === "AI Chat workspace");
	const input = label?.parentElement?.querySelector("input");
	if (!input) throw new Error("AI Chat workspace input not found");
	return input;
}

describe("GeneralTab AI Chat workspace", () => {
	beforeEach(() => {
		settingsStore.setAiChatWorkspace("");
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_home_directory") return Promise.resolve("/home/somebody");
			return Promise.resolve(cmd === "load_config" ? {} : undefined);
		});
	});

	afterEach(() => {
		settingsStore._testCancelPendingSave();
		cleanup();
	});

	it("shows the saved value and saves an edit", () => {
		settingsStore.setAiChatWorkspace("/srv/chat");
		const { container } = render(() => <GeneralTab />);
		const input = aiChatWorkspaceInput(container);
		expect(input.value).toBe("/srv/chat");
		fireEvent.input(input, { target: { value: "/srv/other" } });
		expect(settingsStore.state.aiChatWorkspace).toBe("/srv/other");
	});

	it("shows the host home directory as the effective default", async () => {
		settingsStore.setAiChatWorkspace("");
		const { container } = render(() => <GeneralTab />);
		await waitFor(() => expect(aiChatWorkspaceInput(container).placeholder).toBe("/home/somebody"));
	});

	/** Types the way a person does: each character lands after what the field shows now. */
	function typeSlowly(input: HTMLInputElement, text: string): void {
		for (const char of text) fireEvent.input(input, { target: { value: input.value + char } });
	}

	// Catches: reverting the input on a keystroke that is only invalid mid-typing,
	// which makes "C:\\Users\\x" impossible to type by hand.
	it("saves a Windows path typed one character at a time", () => {
		const { container } = render(() => <GeneralTab />);
		const input = aiChatWorkspaceInput(container);
		typeSlowly(input, "C:\\Users\\x");
		expect(input.value).toBe("C:\\Users\\x");
		expect(settingsStore.state.aiChatWorkspace).toBe("C:\\Users\\x");
		expect(container.textContent).not.toContain("must be an absolute path");
	});

	// Catches: a relative path saved, or refused without telling the person why.
	it("keeps a relative path as typed, saves nothing, and says why on blur", () => {
		settingsStore.setAiChatWorkspace("/srv/chat");
		const { container } = render(() => <GeneralTab />);
		const input = aiChatWorkspaceInput(container);
		// Select-all and type over the saved path.
		fireEvent.input(input, { target: { value: "f" } });
		typeSlowly(input, "oo");
		expect(input.value).toBe("foo");
		expect(container.textContent).not.toContain("must be an absolute path");
		fireEvent.blur(input);
		expect(container.textContent).toContain("must be an absolute path");
		expect(settingsStore.state.aiChatWorkspace).toBe("/srv/chat");
	});

	it("drops the hint once the draft becomes absolute", () => {
		const { container } = render(() => <GeneralTab />);
		const input = aiChatWorkspaceInput(container);
		fireEvent.input(input, { target: { value: "foo" } });
		fireEvent.blur(input);
		fireEvent.input(input, { target: { value: "/foo" } });
		fireEvent.blur(input);
		expect(container.textContent).not.toContain("must be an absolute path");
		expect(settingsStore.state.aiChatWorkspace).toBe("/foo");
	});
});
