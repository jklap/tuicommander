import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

describe("dictationStore", () => {
	let store: typeof import("../../stores/dictation").dictationStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		store = (await import("../../stores/dictation")).dictationStore;
	});

	describe("defaults", () => {
		it("has correct default state", () => {
			testInScope(() => {
				expect(store.state.enabled).toBe(false);
				expect(store.state.hotkey).toBe("F5");
				expect(store.state.language).toBe("auto");
				expect(store.state.selectedModel).toBe("large-v3-turbo");
				expect(store.state.selectedDevice).toBeNull();
				expect(store.state.models).toEqual([]);
				expect(store.state.modelStatus).toBe("not_downloaded");
				expect(store.state.recording).toBe(false);
				expect(store.state.processing).toBe(false);
				expect(store.state.loading).toBe(false);
				expect(store.state.downloading).toBe(false);
				expect(store.state.downloadPercent).toBe(0);
			});
		});
	});

	describe("refreshConfig()", () => {
		it("loads config including model and device fields from backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F6",
				language: "en",
				model: "small",
				device: "USB Microphone",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(mockInvoke).toHaveBeenCalledWith("get_dictation_config");
				expect(store.state.enabled).toBe(true);
				expect(store.state.hotkey).toBe("F6");
				expect(store.state.language).toBe("en");
				expect(store.state.selectedModel).toBe("small");
				expect(store.state.selectedDevice).toBe("USB Microphone");
			});
		});

		it("keeps default model when config has no model field", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: false,
				hotkey: "F5",
				language: "auto",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.selectedModel).toBe("large-v3-turbo");
			});
		});

		it("defaults device to null when config has no device field", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: false,
				hotkey: "F5",
				language: "auto",
				model: "large-v3-turbo",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.selectedDevice).toBeNull();
			});
		});
	});

	describe("refreshModels()", () => {
		it("fetches model info from backend", async () => {
			const mockModels = [
				{ name: "small", display_name: "Whisper Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
				{
					name: "large-v3-turbo",
					display_name: "Whisper Large V3 Turbo",
					size_hint_mb: 1620,
					downloaded: true,
					actual_size_mb: 1620,
				},
			];
			mockInvoke.mockResolvedValueOnce(mockModels);

			await testInScopeAsync(async () => {
				await store.refreshModels();
				expect(mockInvoke).toHaveBeenCalledWith("get_model_info");
				expect(store.state.models).toEqual(mockModels);
			});
		});

		it("handles backend errors gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("backend down"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshModels();
				expect(store.state.models).toEqual([]);
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("setModel()", () => {
		it("saves model to config and updates selectedModel", async () => {
			// First call: get_dictation_config returns current config
			mockInvoke.mockResolvedValueOnce(undefined); // set_dictation_config

			await testInScopeAsync(async () => {
				await store.setModel("small");
				expect(store.state.selectedModel).toBe("small");
				// saveConfig is called with model included
				expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
					config: expect.objectContaining({ model: "small" }),
				});
			});
		});
	});

	describe("deleteModel()", () => {
		it("calls delete_whisper_model and refreshes models", async () => {
			const mockModels = [
				{ name: "small", display_name: "Whisper Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
			];
			// First call: delete_whisper_model
			mockInvoke.mockResolvedValueOnce("Deleted Whisper Small");
			// Second call: get_model_info (from refreshModels)
			mockInvoke.mockResolvedValueOnce(mockModels);

			await testInScopeAsync(async () => {
				await store.deleteModel("small");
				expect(mockInvoke).toHaveBeenCalledWith("delete_whisper_model", { modelName: "small" });
				expect(mockInvoke).toHaveBeenCalledWith("get_model_info");
				expect(store.state.models).toEqual(mockModels);
			});
		});

		it("handles deletion errors gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("file locked"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.deleteModel("small");
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("downloadModel()", () => {
		it("accepts model name parameter", async () => {
			mockInvoke
				.mockResolvedValueOnce("Downloaded") // download_whisper_model
				.mockResolvedValueOnce({
					// get_dictation_status (from refreshStatus)
					model_status: "downloaded",
					model_name: "small",
					model_size_mb: 488,
					recording: false,
					processing: false,
				})
				.mockResolvedValueOnce([]); // get_model_info (from refreshModels)

			await testInScopeAsync(async () => {
				await store.downloadModel("small");
				expect(mockInvoke).toHaveBeenCalledWith("download_whisper_model", { modelName: "small" });
				expect(store.state.downloading).toBe(false);
			});
		});

		it("uses selectedModel when no name provided", async () => {
			mockInvoke
				.mockResolvedValueOnce("Downloaded")
				.mockResolvedValueOnce({
					model_status: "downloaded",
					model_name: "large-v3-turbo",
					model_size_mb: 1620,
					recording: false,
					processing: false,
				})
				.mockResolvedValueOnce([]);

			await testInScopeAsync(async () => {
				await store.downloadModel();
				expect(mockInvoke).toHaveBeenCalledWith("download_whisper_model", { modelName: "large-v3-turbo" });
			});
		});

		it("sets downloading state during download", async () => {
			let resolveDownload: (v: string) => void;
			const downloadPromise = new Promise<string>((r) => {
				resolveDownload = r;
			});
			mockInvoke.mockReturnValueOnce(downloadPromise);

			await testInScopeAsync(async () => {
				const downloadTask = store.downloadModel("small");
				// downloading should be true while in progress
				expect(store.state.downloading).toBe(true);
				expect(store.state.downloadPercent).toBe(0);

				// Resolve the download
				mockInvoke.mockResolvedValueOnce({
					model_status: "downloaded",
					model_name: "small",
					model_size_mb: 488,
					recording: false,
					processing: false,
				});
				mockInvoke.mockResolvedValueOnce([]);
				resolveDownload!("Downloaded");
				await downloadTask;

				expect(store.state.downloading).toBe(false);
			});
		});
	});

	describe("startRecording()", () => {
		it("sets loading=true while invoke is pending and clears it after", async () => {
			let resolveStart: () => void;
			const startPromise = new Promise<void>((r) => {
				resolveStart = r;
			});
			mockInvoke.mockReturnValueOnce(startPromise);

			await testInScopeAsync(async () => {
				const task = store.startRecording();
				expect(store.state.loading).toBe(true);
				expect(store.state.recording).toBe(false);

				resolveStart!();
				await task;

				expect(store.state.loading).toBe(false);
				expect(store.state.recording).toBe(true);
			});
		});

		it("clears loading on failure and rethrows", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("mic busy"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await expect(store.startRecording()).rejects.toThrow("mic busy");
				expect(store.state.loading).toBe(false);
				expect(store.state.recording).toBe(false);
				consoleSpy.mockRestore();
			});
		});
	});

	describe("saveConfig()", () => {
		it("includes model and device in config when saving", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				await store.saveConfig({ language: "en" });
				expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
					config: expect.objectContaining({
						model: "large-v3-turbo",
						language: "en",
						device: null,
					}),
				});
			});
		});

		it("handles save failure gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("disk full"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.saveConfig({ enabled: true });
				expect(consoleSpy).toHaveBeenCalledWith(
					"[dictation]",
					expect.stringContaining("Failed to save"),
					expect.anything(),
				);
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshConfig() error handling", () => {
		it("handles backend error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("backend down"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.enabled).toBe(false); // unchanged
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("stopRecording()", () => {
		it("returns TranscribeResponse on success", async () => {
			// startRecording sets recording=true; mock both start and stop invoke calls
			mockInvoke
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockResolvedValueOnce({
					// stop_dictation_and_transcribe
					text: "Hello world",
					skip_reason: null,
					duration_s: 2.5,
					truncated_s: 0,
				});

			await testInScopeAsync(async () => {
				await store.startRecording();
				expect(store.state.recording).toBe(true);

				const result = await store.stopRecording();
				expect(result).toEqual({
					text: "Hello world",
					skip_reason: null,
					duration_s: 2.5,
					truncated_s: 0,
				});
				expect(store.state.recording).toBe(false);
				expect(store.state.processing).toBe(false);
				expect(store.state.partialText).toBe("");
				expect(mockInvoke).toHaveBeenCalledWith("stop_dictation_and_transcribe");
			});
		});

		it("returns null and resets state on failure", async () => {
			mockInvoke
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockRejectedValueOnce(new Error("transcription failed")); // stop
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.startRecording();
				const result = await store.stopRecording();
				expect(result).toBeNull();
				expect(store.state.recording).toBe(false);
				expect(store.state.processing).toBe(false);
				consoleSpy.mockRestore();
			});
		});

		it("returns null immediately when not recording", async () => {
			await testInScopeAsync(async () => {
				const result = await store.stopRecording();
				expect(result).toBeNull();
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		it("records why a recording produced no text, and clears it on the next success", async () => {
			// The tuning panel has nothing else to show: a gate that rejected
			// speech and a dead microphone both produce an empty transcript.
			mockInvoke
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockResolvedValueOnce({
					text: "",
					skip_reason: "no speech detected (no_speech 0.91 > 0.60)",
					duration_s: 3.1,
					truncated_s: 0,
				})
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockResolvedValueOnce({
					text: "run the tests",
					skip_reason: null,
					duration_s: 1.4,
					truncated_s: 0,
				});

			await testInScopeAsync(async () => {
				await store.startRecording();
				await store.stopRecording();
				expect(store.state.lastSkipReason).toBe("no speech detected (no_speech 0.91 > 0.60)");

				await store.startRecording();
				await store.stopRecording();
				expect(store.state.lastSkipReason).toBeNull();
			});
		});
	});

	describe("voice gates", () => {
		it("defaults to the thresholds the Rust side uses", () => {
			testInScope(() => {
				expect(store.state.rmsThreshold).toBe(0.001);
				expect(store.state.noSpeechThreshold).toBe(0.6);
			});
		});

		it("loads tuned thresholds from the backend config", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F5",
				language: "auto",
				rms_threshold: 0.004,
				no_speech_threshold: 0.35,
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.rmsThreshold).toBe(0.004);
				expect(store.state.noSpeechThreshold).toBe(0.35);
			});
		});

		it("falls back to the defaults when the stored config predates the gates", async () => {
			// A `no_speech_threshold` read as undefined would reach the slider as
			// 0 and reject every transcription.
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F5",
				language: "auto",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.rmsThreshold).toBe(0.001);
				expect(store.state.noSpeechThreshold).toBe(0.6);
			});
		});

		it("persists a tuned threshold without dropping the other one", async () => {
			await testInScopeAsync(async () => {
				await store.saveConfig({ no_speech_threshold: 0.4 });

				expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
					config: expect.objectContaining({
						no_speech_threshold: 0.4,
						rms_threshold: 0.001,
					}),
				});
				expect(store.state.noSpeechThreshold).toBe(0.4);
			});
		});
	});

	describe("injectText()", () => {
		it("returns injected text on success", async () => {
			mockInvoke.mockResolvedValueOnce("corrected text");

			await testInScopeAsync(async () => {
				const result = await store.injectText("raw text");
				expect(result).toBe("corrected text");
				expect(mockInvoke).toHaveBeenCalledWith("inject_text", { text: "raw text" });
			});
		});

		it("returns null on failure", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("inject failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				const result = await store.injectText("raw text");
				expect(result).toBeNull();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshStatus()", () => {
		it("loads status from backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				model_status: "ready",
				model_name: "large-v3-turbo",
				model_size_mb: 1620,
				recording: true,
				processing: false,
			});

			await testInScopeAsync(async () => {
				await store.refreshStatus();
				expect(store.state.modelStatus).toBe("ready");
				expect(store.state.modelName).toBe("large-v3-turbo");
				expect(store.state.modelSizeMb).toBe(1620);
				expect(store.state.recording).toBe(true);
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshStatus();
				expect(store.state.modelStatus).toBe("not_downloaded"); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshCorrections()", () => {
		it("loads correction map from backend", async () => {
			mockInvoke.mockResolvedValueOnce({ hello: "hi", teh: "the" });

			await testInScopeAsync(async () => {
				await store.refreshCorrections();
				expect(store.state.corrections).toEqual({ hello: "hi", teh: "the" });
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshCorrections();
				expect(store.state.corrections).toEqual({}); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("saveCorrections()", () => {
		it("saves corrections to backend", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				await store.saveCorrections({ foo: "bar" });
				expect(mockInvoke).toHaveBeenCalledWith("set_correction_map", { map: { foo: "bar" } });
				expect(store.state.corrections).toEqual({ foo: "bar" });
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.saveCorrections({ foo: "bar" });
				expect(store.state.corrections).toEqual({}); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshDevices()", () => {
		it("loads devices from backend", async () => {
			const devices = [{ name: "Default", is_default: true }];
			mockInvoke.mockResolvedValueOnce(devices);

			await testInScopeAsync(async () => {
				await store.refreshDevices();
				expect(store.state.devices).toEqual(devices);
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshDevices();
				expect(store.state.devices).toEqual([]); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("setEnabled()", () => {
		it("saves config with enabled flag", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				// The setters are fire-and-forget by design, and `saveConfig`
				// now reads the stored config before writing it, so the save
				// lands a microtask later rather than inside the call.
				store.setEnabled(true);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ enabled: true }) }),
					),
				);
			});
		});
	});

	describe("setHotkey()", () => {
		it("saves config with new hotkey", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				store.setHotkey("F8");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ hotkey: "F8" }) }),
					),
				);
			});
		});
	});

	describe("setCapturingHotkey()", () => {
		it("sets capturing state", () => {
			testInScope(() => {
				store.setCapturingHotkey(true);
				expect(store.state.capturingHotkey).toBe(true);
				store.setCapturingHotkey(false);
				expect(store.state.capturingHotkey).toBe(false);
			});
		});
	});

	describe("setLanguage()", () => {
		it("saves config with new language", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				store.setLanguage("fr");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ language: "fr" }) }),
					),
				);
			});
		});
	});

	describe("setDevice()", () => {
		it("saves config with specific device", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				store.setDevice("USB Microphone");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({
							config: expect.objectContaining({ device: "USB Microphone" }),
						}),
					),
				);
			});
		});

		it("saves null device to use system default", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				store.setDevice(null);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ device: null }) }),
					),
				);
			});
		});
	});

	describe("setNotifyModelOnHandsFree()", () => {
		/**
		 * The one setting whose default is `true`, so the assertion that
		 * matters is the one that turns it off: a default-on flag that cannot
		 * be written false is indistinguishable from a flag nobody reads.
		 */
		it("writes the flag off and remembers it", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);

			await testInScopeAsync(async () => {
				expect(store.state.notifyModelOnHandsFree, "on by default, as in Rust").toBe(true);
				store.setNotifyModelOnHandsFree(false);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({
							config: expect.objectContaining({ hands_free_notify_model: false }),
						}),
					),
				);
				await vi.waitFor(() => expect(store.state.notifyModelOnHandsFree).toBe(false));
			});
		});
	});

	describe("downloadModel() error", () => {
		it("clears downloading state on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("download failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.downloadModel("small");
				expect(store.state.downloading).toBe(false);
				consoleSpy.mockRestore();
			});
		});
	});

	/**
	 * `set_dictation_config` takes the whole config object as its body and
	 * `save` writes it wholesale, so a caller that rebuilds that object from a
	 * hand-written list resets every field it forgot. Rust cannot refuse the
	 * payload: every hands-free field carries `#[serde(default)]`, because a
	 * config written before the field existed has to load. So a dropped field
	 * is not an error anywhere — the setting just silently reverts.
	 *
	 * It has already happened twice. `hands_free_hold_back_ms` (814-6d13) has
	 * been snapping back to its default since it was added, and
	 * `hands_free_activation_phrase` (815-7c76) would have done the same, which
	 * is worse: a gate the user configured and the UI quietly disarmed.
	 *
	 * The fix is the load-modify-save rule already recorded for `save_config`:
	 * read the stored config, change only what this surface owns, write it
	 * back. So the assertion is not "TypeScript lists the same fields as Rust"
	 * — under load-modify-save it does not have to. It is the weaker and more
	 * durable "a field the UI does not model survives a save", driven off the
	 * Rust struct so the twelfth field is covered by the person who adds it
	 * rather than by the person who later forgets it.
	 */
	describe("saveConfig() payload", () => {
		/** Field names declared by `DictationConfig` in the Rust source. */
		function rustConfigFields(): string[] {
			const source = readFileSync(join(process.cwd(), "src-tauri/src/dictation/commands.rs"), "utf8");
			const struct = source.match(/pub struct DictationConfig \{([\s\S]*?)\n\}/);
			if (!struct) throw new Error("DictationConfig not found in commands.rs");
			const fields = [...struct[1].matchAll(/^\s*pub ([a-z0-9_]+):/gm)].map((match) => match[1]);
			if (fields.length === 0) throw new Error("DictationConfig parsed to zero fields");
			return fields;
		}

		it("carries every field the Rust struct declares, including ones the UI never models", async () => {
			const fields = rustConfigFields();
			// The stored config as Rust would hand it back: every declared
			// field present, each with a value this test can recognise again.
			const stored = Object.fromEntries(fields.map((field) => [field, `stored:${field}`]));
			mockInvoke.mockReset();
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(command === "get_dictation_config" ? stored : undefined),
			);

			await testInScopeAsync(async () => {
				await store.saveConfig({ auto_send: true });

				const call = mockInvoke.mock.calls.find(([name]) => name === "set_dictation_config");
				if (!call) throw new Error("saveConfig must reach set_dictation_config");
				const sent = (call[1] as { config: Record<string, unknown> }).config;

				expect(Object.keys(sent).sort()).toEqual([...fields].sort());
				expect(sent.auto_send, "the caller's own change must win").toBe(true);
				expect(sent.hands_free_activation_phrase, "a field no UI control models must survive untouched").toBe(
					"stored:hands_free_activation_phrase",
				);
				expect(sent.hands_free_hold_back_ms).toBe("stored:hands_free_hold_back_ms");
			});
		});

		it("does not write a config it could not read first", async () => {
			mockInvoke.mockReset();
			mockInvoke.mockImplementation((command: string) =>
				command === "get_dictation_config" ? Promise.reject(new Error("backend down")) : Promise.resolve(undefined),
			);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.saveConfig({ auto_send: true });

				expect(
					mockInvoke.mock.calls.some(([name]) => name === "set_dictation_config"),
					"a failed load must abort the save, not write a config built from defaults",
				).toBe(false);
				consoleSpy.mockRestore();
			});
		});
	});
});
