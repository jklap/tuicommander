import { createRoot } from "solid-js";
import { createStore } from "solid-js/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ release: null as null | ((ok: boolean) => void) }));

vi.mock("../../transport", () => ({ rpc: vi.fn().mockResolvedValue({}) }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { info: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/dictation", () => {
	const [state, setState] = createStore<{ handsFree: unknown; handsFreeError: string | null }>({
		handsFree: null,
		handsFreeError: null,
	});
	const arm = vi.fn(
		(sessionId: string) =>
			new Promise<boolean>((resolve) => {
				h.release = (ok) => {
					if (ok) setState("handsFree", { armed: true, sessionId, owner: "browser-x", phase: "waiting" });
					resolve(ok);
				};
			}),
	);
	const disarm = vi.fn(async () => setState("handsFree", { armed: false }));
	return { browserAudioOwner: "browser-x", dictationStore: { state, armHandsFree: arm, disarmHandsFree: disarm } };
});

import { dictationStore } from "../../stores/dictation";
import { useMobileVoice } from "../useMobileVoice";

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("useMobileVoice critic", () => {
	beforeEach(() => vi.clearAllMocks());

	it("catches: leaving the screen while the mic prompt is pending leaves the mic open on a hidden page", async () => {
		let dispose = () => {};
		let voice!: ReturnType<typeof useMobileVoice>;
		createRoot((d) => {
			dispose = d;
			voice = useMobileVoice(
				() => "s1",
				() => true,
			);
		});
		await flush();
		await flush();
		expect(voice.available()).toBe(true);
		const pending = voice.toggle();
		dispose(); // user presses Back while getUserMedia prompt is open
		h.release?.(true); // permission granted afterwards
		await pending;
		await flush();
		expect(dictationStore.disarmHandsFree).toHaveBeenCalled();
	});

	it("catches: a second tap during arming opens a second conversation", async () => {
		let voice!: ReturnType<typeof useMobileVoice>;
		createRoot(() => {
			voice = useMobileVoice(
				() => "s1",
				() => true,
			);
		});
		await flush();
		await flush();
		void voice.toggle();
		void voice.toggle();
		expect(dictationStore.armHandsFree).toHaveBeenCalledTimes(1);
	});
});
