import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";

const {
	mockSetEnabled,
	mockSetVolume,
	mockSetAudioDevice,
	mockSetSilenceRemoteCompletions,
	mockSetToastsInBell,
	mockSetSoundEnabled,
	mockTestSound,
	mockReset,
} = vi.hoisted(() => ({
	mockSetEnabled: vi.fn(),
	mockSetVolume: vi.fn(),
	mockSetAudioDevice: vi.fn(),
	mockSetSilenceRemoteCompletions: vi.fn(),
	mockSetToastsInBell: vi.fn(),
	mockSetSoundEnabled: vi.fn(),
	mockTestSound: vi.fn(),
	mockReset: vi.fn(),
}));

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
			},
		},
		setEnabled: mockSetEnabled,
		setVolume: mockSetVolume,
		setAudioDevice: mockSetAudioDevice,
		setSilenceRemoteCompletions: mockSetSilenceRemoteCompletions,
		setToastsInBell: mockSetToastsInBell,
		setSoundEnabled: mockSetSoundEnabled,
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
