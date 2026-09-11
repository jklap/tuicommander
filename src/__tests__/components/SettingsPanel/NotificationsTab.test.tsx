import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../mocks/tauri";

const {
	mockSetEnabled,
	mockSetVolume,
	mockSetAudioDevice,
	mockSetSilenceRemoteCompletions,
	mockSetToastsInBell,
	mockSetSoundEnabled,
	mockSetSoundChoice,
	mockTestSound,
	mockReset,
} = vi.hoisted(() => ({
	mockSetEnabled: vi.fn(),
	mockSetVolume: vi.fn(),
	mockSetAudioDevice: vi.fn(),
	mockSetSilenceRemoteCompletions: vi.fn(),
	mockSetToastsInBell: vi.fn(),
	mockSetSoundEnabled: vi.fn(),
	mockSetSoundChoice: vi.fn(),
	mockTestSound: vi.fn(),
	mockReset: vi.fn(),
}));

function defaultChoice() {
	return { preset: "default" as const, custom_path: null };
}

vi.mock("../../../stores/notifications", () => ({
	notificationsStore: {
		state: {
			isAvailable: true,
			config: {
				enabled: true,
				volume: 0.5,
				audio_device: null,
				silence_remote_completions: false,
				toasts_in_bell: true,
				sounds: {
					question: true,
					error: true,
					completion: true,
					warning: false,
					info: true,
					attention: true,
				},
				sound_choices: {
					question: defaultChoice(),
					error: defaultChoice(),
					completion: defaultChoice(),
					warning: defaultChoice(),
					info: defaultChoice(),
					attention: defaultChoice(),
				},
			},
		},
		setEnabled: mockSetEnabled,
		setVolume: mockSetVolume,
		setAudioDevice: mockSetAudioDevice,
		setSilenceRemoteCompletions: mockSetSilenceRemoteCompletions,
		setToastsInBell: mockSetToastsInBell,
		setSoundEnabled: mockSetSoundEnabled,
		setSoundChoice: mockSetSoundChoice,
		testSound: mockTestSound,
		reset: mockReset,
	},
}));

import { NotificationsTab } from "../../../components/SettingsPanel/tabs/NotificationsTab";

describe("NotificationsTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders the Notification Settings heading and per-event sound rows", () => {
		const { container, getByText } = render(() => <NotificationsTab />);
		expect(getByText("Notification Settings")).toBeTruthy();
		const soundLabels = ["Question", "Error", "Completion", "Warning", "Info", "Attention (agent needs you)"];
		for (const label of soundLabels) {
			expect(getByText(label)).toBeTruthy();
		}
		// Master toggle + 6 per-event toggles + orchestration toggle + bell toggle = 9
		expect(container.querySelectorAll('input[type="checkbox"]')).toHaveLength(9);
	});

	it("calls setEnabled when the master toggle changes", () => {
		const { container } = render(() => <NotificationsTab />);
		const master = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.change(master, { target: { checked: false } });
		expect(mockSetEnabled).toHaveBeenCalledWith(false);
	});

	it("calls setSoundEnabled with the right sound key when a per-event toggle changes", () => {
		const { container } = render(() => <NotificationsTab />);
		const checkboxes = Array.from(container.querySelectorAll('input[type="checkbox"]'));
		// checkboxes[0] is the master toggle; the six sound rows follow in declared order.
		fireEvent.change(checkboxes[4], { target: { checked: false } }); // "warning"
		expect(mockSetSoundEnabled).toHaveBeenCalledWith("warning", false);
	});

	it("calls testSound when a sound row's Test button is clicked", () => {
		const { getAllByText } = render(() => <NotificationsTab />);
		const testButtons = getAllByText("Test");
		fireEvent.click(testButtons[0]);
		expect(mockTestSound).toHaveBeenCalledWith("question");
	});

	it("calls setSilenceRemoteCompletions when the orchestration toggle changes", () => {
		const { getByText } = render(() => <NotificationsTab />);
		const label = getByText("Silence completions from MCP sessions");
		const checkbox = label.closest("div")?.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.change(checkbox, { target: { checked: true } });
		expect(mockSetSilenceRemoteCompletions).toHaveBeenCalledWith(true);
	});

	it("calls setToastsInBell when the bell toggle changes, and renders it outside the audio-availability gate", () => {
		const { getByText } = render(() => <NotificationsTab />);
		const label = getByText("Keep toasts in the bell");
		const checkbox = label.closest("div")?.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.change(checkbox, { target: { checked: false } });
		expect(mockSetToastsInBell).toHaveBeenCalledWith(false);
	});

	it("calls reset when Reset Defaults is clicked", () => {
		const { getByText } = render(() => <NotificationsTab />);
		fireEvent.click(getByText("Reset Defaults"));
		expect(mockReset).toHaveBeenCalledOnce();
	});

	it("calls setVolume with a 0-1 fraction as the slider is dragged", () => {
		const { container } = render(() => <NotificationsTab />);
		const slider = container.querySelector('input[type="range"]') as HTMLInputElement;
		fireEvent.input(slider, { target: { value: "80" } });
		expect(mockSetVolume).toHaveBeenCalledWith(0.8);
	});

	it("plays an 'info' preview once the volume slider is released", () => {
		const { container } = render(() => <NotificationsTab />);
		const slider = container.querySelector('input[type="range"]') as HTMLInputElement;
		fireEvent.change(slider, { target: { value: "80" } });
		expect(mockTestSound).toHaveBeenCalledWith("info");
		// Releasing the slider must not also fire setVolume a second time —
		// that already happened live via onInput.
		expect(mockSetVolume).not.toHaveBeenCalled();
	});

	describe("audio output device picker", () => {
		beforeEach(() => {
			mockInvoke.mockReset();
		});

		it("lazily loads devices only when 'Choose output device…' is clicked, not on mount", () => {
			render(() => <NotificationsTab />);
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		/** Per-sound preset pickers are also plain `<select>`s now, so a bare
		 *  `container.querySelector("select")` is ambiguous — scope to the
		 *  device group specifically via its label. */
		function findDeviceSelect(getByText: (text: string) => HTMLElement): HTMLSelectElement {
			const label = getByText("Audio Output Device");
			const el = label.closest("div")?.querySelector("select");
			if (!el) throw new Error("device select not yet rendered");
			return el as HTMLSelectElement;
		}

		it("lists devices returned by list_audio_output_devices, marking the default", async () => {
			mockInvoke.mockResolvedValueOnce([
				{ name: "Built-in Speakers", is_default: true },
				{ name: "USB Headset", is_default: false },
			]);
			const { getByText } = render(() => <NotificationsTab />);
			fireEvent.click(getByText("Choose output device…"));

			expect(mockInvoke).toHaveBeenCalledWith("list_audio_output_devices");
			const select = await waitFor(() => findDeviceSelect(getByText));
			const options = Array.from(select.querySelectorAll("option")).map((o) => o.textContent);
			expect(options).toEqual(["System Default", "Built-in Speakers (current default)", "USB Headset"]);
		});

		it("calls setAudioDevice with the selected device name", async () => {
			mockInvoke.mockResolvedValueOnce([{ name: "USB Headset", is_default: false }]);
			const { getByText } = render(() => <NotificationsTab />);
			fireEvent.click(getByText("Choose output device…"));

			const select = await waitFor(() => findDeviceSelect(getByText));
			fireEvent.change(select, { target: { value: "USB Headset" } });
			expect(mockSetAudioDevice).toHaveBeenCalledWith("USB Headset");
		});

		it("calls setAudioDevice with null when 'System Default' is re-selected", async () => {
			mockInvoke.mockResolvedValueOnce([{ name: "USB Headset", is_default: false }]);
			const { getByText } = render(() => <NotificationsTab />);
			fireEvent.click(getByText("Choose output device…"));

			const select = await waitFor(() => findDeviceSelect(getByText));
			fireEvent.change(select, { target: { value: "" } });
			expect(mockSetAudioDevice).toHaveBeenCalledWith(null);
		});

		it("falls back to an empty (System Default-only) list when enumeration fails, instead of throwing", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("mic permission denied"));
			const { getByText } = render(() => <NotificationsTab />);
			fireEvent.click(getByText("Choose output device…"));

			const select = await waitFor(() => findDeviceSelect(getByText));
			expect(select.querySelectorAll("option")).toHaveLength(1);
			expect(select.querySelector("option")?.textContent).toBe("System Default");
		});
	});

	describe("per-sound preset picker", () => {
		function presetSelectFor(container: HTMLElement, label: string): HTMLSelectElement {
			const row = Array.from(container.querySelectorAll<HTMLElement>("div")).find(
				(div) => div.textContent?.includes(label) && div.querySelector("select"),
			);
			const el = row?.querySelector("select");
			if (!el) throw new Error(`preset select for "${label}" not found`);
			return el as HTMLSelectElement;
		}

		it("offers every OTHER sound's tone as a preset, plus Default and Custom file…, but not its own tone", () => {
			const { container } = render(() => <NotificationsTab />);
			const select = presetSelectFor(container, "Question");
			const options = Array.from(select.querySelectorAll("option")).map((o) => o.textContent);
			expect(options).toEqual(["Default", "Low Tone", "Arpeggio", "Double-Tap", "Pluck", "Callback", "Custom file…"]);
		});

		it("selecting another sound's preset calls setSoundChoice with that preset and no custom path", () => {
			const { container } = render(() => <NotificationsTab />);
			const select = presetSelectFor(container, "Question");
			fireEvent.change(select, { target: { value: "attention" } });
			expect(mockSetSoundChoice).toHaveBeenCalledWith("question", { preset: "attention", custom_path: null });
		});

		it("selecting Custom file… opens the native file picker and, on a pick, calls setSoundChoice with the path", async () => {
			const { open } = await import("@tauri-apps/plugin-dialog");
			vi.mocked(open).mockResolvedValueOnce("/Users/me/sounds/ding.wav");
			const { container } = render(() => <NotificationsTab />);
			const select = presetSelectFor(container, "Question");

			fireEvent.change(select, { target: { value: "custom" } });
			await waitFor(() =>
				expect(mockSetSoundChoice).toHaveBeenCalledWith("question", {
					preset: "custom",
					custom_path: "/Users/me/sounds/ding.wav",
				}),
			);
		});

		it("canceling the file picker (null) leaves the choice unchanged and does not call setSoundChoice", async () => {
			const { open } = await import("@tauri-apps/plugin-dialog");
			vi.mocked(open).mockResolvedValueOnce(null);
			const { container } = render(() => <NotificationsTab />);
			const select = presetSelectFor(container, "Question");

			fireEvent.change(select, { target: { value: "custom" } });
			await waitFor(() => expect(vi.mocked(open)).toHaveBeenCalled());
			expect(mockSetSoundChoice).not.toHaveBeenCalled();
			// The visible selection reverts to what the store still holds ("default").
			expect(select.value).toBe("default");
		});

		it("a rejected file picker also reverts the visible selection instead of leaving it stuck on 'custom'", async () => {
			const { open } = await import("@tauri-apps/plugin-dialog");
			vi.mocked(open).mockRejectedValueOnce(new Error("dialog plugin unavailable"));
			const { container } = render(() => <NotificationsTab />);
			const select = presetSelectFor(container, "Question");

			fireEvent.change(select, { target: { value: "custom" } });
			await waitFor(() => expect(select.value).toBe("default"));
			expect(mockSetSoundChoice).not.toHaveBeenCalled();
		});
	});
});

describe("NotificationsTab (platform without audio)", () => {
	let NotificationsTabUnavailable: typeof import("../../../components/SettingsPanel/tabs/NotificationsTab").NotificationsTab;

	beforeEach(async () => {
		vi.resetModules();
		vi.doMock("../../../stores/notifications", () => ({
			notificationsStore: {
				state: { isAvailable: false, config: { sounds: {}, toasts_in_bell: true } },
				setEnabled: vi.fn(),
				setVolume: vi.fn(),
				setAudioDevice: vi.fn(),
				setSilenceRemoteCompletions: vi.fn(),
				setToastsInBell: vi.fn(),
				setSoundEnabled: vi.fn(),
				testSound: vi.fn(),
				reset: vi.fn(),
			},
		}));
		const mod = await import("../../../components/SettingsPanel/tabs/NotificationsTab");
		NotificationsTabUnavailable = mod.NotificationsTab;
	});

	it("shows the not-available warning and hides the audio controls, but keeps the bell toggle reachable", () => {
		const { getByText, queryByText } = render(() => <NotificationsTabUnavailable />);
		expect(getByText("Audio notifications are not available on this platform")).toBeTruthy();
		expect(queryByText("Enable audio notifications")).toBeNull();
		expect(queryByText("Reset Defaults")).toBeNull();
		// The bell setting lives outside the audio Show gate deliberately (it's visual, not audio).
		expect(getByText("Keep toasts in the bell")).toBeTruthy();
	});
});

describe("NotificationsTab (a sound already has a custom file configured)", () => {
	const mockSetSoundChoiceCustom = vi.fn();
	let NotificationsTabCustom: typeof import("../../../components/SettingsPanel/tabs/NotificationsTab").NotificationsTab;

	beforeEach(async () => {
		vi.resetModules();
		mockSetSoundChoiceCustom.mockClear();
		vi.doMock("../../../stores/notifications", () => ({
			notificationsStore: {
				state: {
					isAvailable: true,
					config: {
						enabled: true,
						volume: 0.5,
						audio_device: null,
						silence_remote_completions: false,
						toasts_in_bell: true,
						sounds: { question: true, error: true, completion: true, warning: true, info: true, attention: true },
						sound_choices: {
							question: { preset: "custom", custom_path: "/Users/me/sounds/ding.wav" },
							error: { preset: "default", custom_path: null },
							completion: { preset: "default", custom_path: null },
							warning: { preset: "default", custom_path: null },
							info: { preset: "default", custom_path: null },
							attention: { preset: "default", custom_path: null },
						},
					},
				},
				setEnabled: vi.fn(),
				setVolume: vi.fn(),
				setAudioDevice: vi.fn(),
				setSilenceRemoteCompletions: vi.fn(),
				setToastsInBell: vi.fn(),
				setSoundEnabled: vi.fn(),
				setSoundChoice: mockSetSoundChoiceCustom,
				testSound: vi.fn(),
				reset: vi.fn(),
			},
		}));
		const mod = await import("../../../components/SettingsPanel/tabs/NotificationsTab");
		NotificationsTabCustom = mod.NotificationsTab;
	});

	it("shows the custom filename and lets the user reset that sound back to default", () => {
		const { getByText } = render(() => <NotificationsTabCustom />);
		expect(getByText("Custom: ding.wav")).toBeTruthy();

		fireEvent.click(getByText("Reset to default"));
		expect(mockSetSoundChoiceCustom).toHaveBeenCalledWith("question", { preset: "default", custom_path: null });
	});
});

describe("NotificationsTab (browser mode viewing a custom choice made on desktop)", () => {
	// Desktop and a browser-mode dev instance share config.json (see AGENTS.md's
	// isolation caveat), so a sound already set to "custom" on desktop is a real
	// state a browser client can observe, even though it can't set or use one.
	const originalTauriShim = (globalThis as Record<string, unknown>).__TAURI_SHIM__;
	let NotificationsTabBrowser: typeof import("../../../components/SettingsPanel/tabs/NotificationsTab").NotificationsTab;

	beforeEach(async () => {
		(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true; // forces isTauri() === false
		vi.resetModules();
		vi.doMock("../../../stores/notifications", () => ({
			notificationsStore: {
				state: {
					isAvailable: true,
					config: {
						enabled: true,
						volume: 0.5,
						audio_device: null,
						silence_remote_completions: false,
						toasts_in_bell: true,
						sounds: { question: true, error: true, completion: true, warning: true, info: true, attention: true },
						sound_choices: {
							question: { preset: "custom", custom_path: "/Users/me/sounds/ding.wav" },
							error: { preset: "default", custom_path: null },
							completion: { preset: "default", custom_path: null },
							warning: { preset: "default", custom_path: null },
							info: { preset: "default", custom_path: null },
							attention: { preset: "default", custom_path: null },
						},
					},
				},
				setEnabled: vi.fn(),
				setVolume: vi.fn(),
				setAudioDevice: vi.fn(),
				setSilenceRemoteCompletions: vi.fn(),
				setToastsInBell: vi.fn(),
				setSoundEnabled: vi.fn(),
				setSoundChoice: vi.fn(),
				testSound: vi.fn(),
				reset: vi.fn(),
			},
		}));
		const mod = await import("../../../components/SettingsPanel/tabs/NotificationsTab");
		NotificationsTabBrowser = mod.NotificationsTab;
	});

	afterEach(() => {
		if (originalTauriShim === undefined) {
			delete (globalThis as Record<string, unknown>).__TAURI_SHIM__;
		} else {
			(globalThis as Record<string, unknown>).__TAURI_SHIM__ = originalTauriShim;
		}
	});

	it("renders a disabled option matching the persisted 'custom' value instead of leaving the select unmatched", () => {
		const { container, getByText } = render(() => <NotificationsTabBrowser />);
		const row = Array.from(container.querySelectorAll<HTMLElement>("div")).find(
			(div) => div.textContent?.includes("Question") && div.querySelector("select"),
		);
		const select = row?.querySelector("select") as HTMLSelectElement;

		expect(select.value).toBe("custom");
		const customOption = Array.from(select.querySelectorAll("option")).find((o) => o.value === "custom");
		expect(customOption?.disabled).toBe(true);
		// The live "Custom file…" picker option must not be offered here.
		expect(customOption?.textContent).not.toBe("Custom file…");
		// The hint + reset control stays reachable regardless of mode.
		expect(getByText("Custom: ding.wav")).toBeTruthy();
	});
});
