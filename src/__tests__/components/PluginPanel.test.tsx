import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginPanelTab } from "../../stores/mdTabs";

// Mock pluginRegistry to avoid Tauri calls
vi.mock("../../plugins/pluginRegistry", () => ({
	pluginRegistry: {
		handlePanelMessage: vi.fn(),
		registerPanelSendChannel: vi.fn(),
		unregisterPanelSendChannel: vi.fn(),
		setPanelVisible: vi.fn(),
	},
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
	})),
}));

const mockWriteClipboard = vi.fn().mockResolvedValue(undefined);
vi.mock("../../utils/clipboard", () => ({ writeClipboard: (text: string) => mockWriteClipboard(text) }));

const mockAppLoggerWarn = vi.fn();
vi.mock("../../stores/appLogger", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../stores/appLogger")>();
	return { ...actual, appLogger: { ...actual.appLogger, warn: (...args: unknown[]) => mockAppLoggerWarn(...args) } };
});

// Track whether addEventListener("message") was called inside an onMount callback.
// We wrap solid-js onMount to set a flag during its execution.
let insideOnMount = false;
let messageListenerCalledInsideOnMount: boolean | null = null;

vi.mock("solid-js", async (importOriginal) => {
	const actual = await importOriginal<typeof import("solid-js")>();
	return {
		...actual,
		onMount: (fn: () => void) => {
			return actual.onMount(() => {
				insideOnMount = true;
				fn();
				insideOnMount = false;
			});
		},
	};
});

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import { injectThemeVars, PluginPanel } from "../../components/PluginPanel/PluginPanel";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { applyAppTheme } from "../../themes";

function makeTab(overrides: Partial<PluginPanelTab> = {}): PluginPanelTab {
	return {
		id: "tab-1",
		type: "plugin-panel",
		pluginId: "test-plugin",
		title: "Test Plugin",
		html: "<html><body>hello</body></html>",
		...overrides,
	} as PluginPanelTab;
}

describe("PluginPanel", () => {
	let addEventListenerSpy: ReturnType<typeof vi.spyOn>;
	let removeEventListenerSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		vi.useFakeTimers();
		insideOnMount = false;
		messageListenerCalledInsideOnMount = null;
		mockWriteClipboard.mockReset().mockResolvedValue(undefined);
		mockAppLoggerWarn.mockReset();

		const originalAdd = window.addEventListener.bind(window);
		addEventListenerSpy = vi
			.spyOn(window, "addEventListener")
			.mockImplementation((event: string, ...rest: unknown[]) => {
				if (event === "message") {
					messageListenerCalledInsideOnMount = insideOnMount;
				}
				return originalAdd(
					event as keyof WindowEventMap,
					...(rest as [EventListenerOrEventListenerObject, (boolean | AddEventListenerOptions)?]),
				);
			});
		removeEventListenerSpy = vi.spyOn(window, "removeEventListener");
	});

	afterEach(async () => {
		vi.runAllTicks();
		vi.advanceTimersByTime(0);
		await vi.runAllTimersAsync();
		vi.useRealTimers();
		addEventListenerSpy.mockRestore();
		removeEventListenerSpy.mockRestore();
	});

	it("addEventListener('message') is called inside onMount — not at component body evaluation", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		// The message listener MUST have been registered inside onMount, not at component body level
		expect(messageListenerCalledInsideOnMount).toBe(true);
	});

	it("registers message listener on window during mount", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		const messageCalls = addEventListenerSpy.mock.calls.filter(([event]: [string]) => event === "message");
		expect(messageCalls).toHaveLength(1);
	});

	it("removes message listener on unmount", () => {
		const tab = makeTab();
		const { unmount } = render(() => <PluginPanel tab={tab} />);

		// Capture which handler was registered
		const [, registeredHandler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;

		unmount();

		const removeCalls = removeEventListenerSpy.mock.calls.filter(([event]: [string]) => event === "message");
		expect(removeCalls).toHaveLength(1);
		expect(removeCalls[0][1]).toBe(registeredHandler);
	});

	it("registers send channel via pluginRegistry on mount", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		expect(pluginRegistry.registerPanelSendChannel).toHaveBeenCalledWith("tab-1", expect.any(Function));
	});

	it("unregisters send channel via pluginRegistry on unmount", () => {
		const tab = makeTab();
		const { unmount } = render(() => <PluginPanel tab={tab} />);
		unmount();

		expect(pluginRegistry.unregisterPanelSendChannel).toHaveBeenCalledWith("tab-1");
	});

	it("routes non-system messages without throwing (source guard prevents routing when no iframe)", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		// Get the registered handler
		const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;

		// Simulate a message from an unknown source — handler guards on iframeRef.contentWindow
		const fakeEvent = new MessageEvent("message", {
			data: { type: "custom", payload: "test" },
		});
		expect(() => (handler as EventListener)(fakeEvent)).not.toThrow();
	});

	it("renders an iframe element", () => {
		const tab = makeTab();
		const { container } = render(() => <PluginPanel tab={tab} />);
		const iframe = container.querySelector("iframe");
		expect(iframe).not.toBeNull();
	});

	it("renders iframe with sandbox allow-scripts allow-same-origin", () => {
		const tab = makeTab();
		const { container } = render(() => <PluginPanel tab={tab} />);
		const iframe = container.querySelector("iframe");
		expect(iframe?.getAttribute("sandbox")).toBe("allow-scripts allow-same-origin");
	});

	describe("srcdoc writes (613-00e8 F100)", () => {
		/**
		 * Count assignments to the iframe's srcdoc. Reassigning it navigates the
		 * iframe: a fresh document, a fresh JS global, and the scroll position,
		 * focus and in-page state of the old one gone. It must happen only when the
		 * HTML actually changed.
		 */
		function countSrcdocWrites(iframe: HTMLIFrameElement) {
			const counter = { writes: 0 };
			let value = iframe.getAttribute("srcdoc") ?? "";
			Object.defineProperty(iframe, "srcdoc", {
				configurable: true,
				get: () => value,
				set: (next: string) => {
					counter.writes++;
					value = next;
				},
			});
			const realSetAttribute = iframe.setAttribute.bind(iframe);
			iframe.setAttribute = (name: string, next: string) => {
				if (name === "srcdoc") {
					counter.writes++;
					value = next;
					return;
				}
				realSetAttribute(name, next);
			};
			return counter;
		}

		it("re-renders the iframe only when the plugin's HTML actually changed", () => {
			mdTabsStore.clearAll();
			const tabId = mdTabsStore.addPluginPanel("p1", "dash", "Dash", "<html><body>v1</body></html>");
			const { container } = render(() => <PluginPanel tab={mdTabsStore.get(tabId) as PluginPanelTab} />);
			const counter = countSrcdocWrites(container.querySelector("iframe") as HTMLIFrameElement);

			mdTabsStore.updatePluginPanel(tabId, "<html><body>v1</body></html>");
			expect(counter.writes).toBe(0);

			mdTabsStore.updatePluginPanel(tabId, "<html><body>v2</body></html>");
			expect(counter.writes).toBe(1);
		});
	});

	describe("hidden panels (613-00e8 F102)", () => {
		/**
		 * Mount a panel and start spying on what reaches its iframe. The stub goes
		 * in after mount on purpose — the mount-time handshake is not what this
		 * block is about.
		 */
		function spyOnPanelTraffic(visible: () => boolean) {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} visible={visible} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const postMessage = vi.fn();
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => ({ postMessage }),
			});
			return postMessage;
		}

		const repoMessages = (postMessage: ReturnType<typeof vi.fn>) =>
			postMessage.mock.calls.filter((call) => call[0]?.type === "tuic:repo-changed");

		afterEach(() => {
			repositoriesStore.setActive(null);
			repositoriesStore._testCancelPendingSave();
		});

		it("does not push repo changes at a panel nobody can see", () => {
			const postMessage = spyOnPanelTraffic(() => false);

			repositoriesStore.setActive("/repo-x");

			expect(repoMessages(postMessage)).toHaveLength(0);
		});

		it("hands the current repo over the moment the panel is shown", () => {
			const [visible, setVisible] = createSignal(false);
			const postMessage = spyOnPanelTraffic(visible);

			repositoriesStore.setActive("/repo-x");
			expect(repoMessages(postMessage)).toHaveLength(0);

			setVisible(true);

			// Exactly one — the panel must learn the current repo, not replay every
			// repo it missed while hidden.
			expect(repoMessages(postMessage)).toHaveLength(1);
			expect(repoMessages(postMessage)[0][0]).toEqual({ type: "tuic:repo-changed", repoPath: "/repo-x" });
		});

		it("keeps pushing at a visible panel", () => {
			const postMessage = spyOnPanelTraffic(() => true);

			repositoriesStore.setActive("/repo-x");

			expect(repoMessages(postMessage)).toHaveLength(1);
		});
	});

	describe("theme extraction (613-00e8 F103)", () => {
		const HTML = "<html><head></head><body></body></html>";
		let sheetReads = 0;

		beforeEach(() => {
			// Invalidate whatever earlier tests left in the cache, so the first
			// injection below is guaranteed to be the one that walks the sheets.
			applyAppTheme("vscode-dark");
			sheetReads = 0;
			// StyleSheetList is live, so holding the object is enough to hand back.
			const real = document.styleSheets;
			Object.defineProperty(document, "styleSheets", {
				configurable: true,
				get() {
					sheetReads++;
					return real;
				},
			});
		});

		afterEach(() => {
			delete (document as unknown as Record<string, unknown>).styleSheets;
		});

		it("walks the stylesheets once and reuses the result for every later injection", () => {
			injectThemeVars(HTML, false);
			const afterFirst = sheetReads;
			expect(afterFirst).toBeGreaterThan(0);

			injectThemeVars(HTML, false);
			injectThemeVars(HTML, true);

			expect(sheetReads).toBe(afterFirst);
		});

		it("re-reads the stylesheets once applyAppTheme has rewritten the root variables", () => {
			injectThemeVars(HTML, false);
			const afterFirst = sheetReads;

			applyAppTheme("vscode-dark");
			injectThemeVars(HTML, false);

			expect(sheetReads).toBeGreaterThan(afterFirst);
		});
	});

	describe("URL mode — SDK handshake", () => {
		function makeUrlTab(): PluginPanelTab {
			return {
				id: "tab-url",
				type: "plugin-panel",
				pluginId: "test-plugin",
				title: "URL Plugin",
				html: "",
				url: "about:blank",
			} as PluginPanelTab;
		}

		it("renders URL iframe with allow-scripts allow-same-origin sandbox", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe");
			expect(iframe?.getAttribute("sandbox")).toBe("allow-scripts allow-same-origin");
			expect(iframe?.getAttribute("src")).toBe("about:blank");
		});

		it("posts tuic:sdk-init to the iframe on load", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			// Stub contentWindow.postMessage — jsdom gives us a real window, we spy on it
			const postMessageSpy = vi.fn();
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => ({ postMessage: postMessageSpy }),
			});

			// Trigger the onLoad handler
			iframe.dispatchEvent(new Event("load"));

			// sendSdkInit posts three messages: sdk-init + repo-changed + theme-changed
			expect(postMessageSpy).toHaveBeenCalledTimes(3);
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:sdk-init", version: "1.0" }, "*");
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:repo-changed", repoPath: null }, "*");
		});

		it("responds to tuic:sdk-request with tuic:sdk-init (async-listener fallback)", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			const postMessageSpy = vi.fn();
			const fakeContentWindow = { postMessage: postMessageSpy };
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => fakeContentWindow,
			});

			// Get the registered window message handler
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;

			// Simulate the child sending tuic:sdk-request after its listener became ready
			const event = new MessageEvent("message", {
				data: { type: "tuic:sdk-request" },
			});
			// Force event.source to match contentWindow so the source guard passes
			Object.defineProperty(event, "source", {
				get: () => fakeContentWindow,
			});

			(handler as EventListener)(event);

			// sendSdkInit posts three messages: sdk-init + repo-changed + theme-changed
			expect(postMessageSpy).toHaveBeenCalledTimes(3);
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:sdk-init", version: "1.0" }, "*");
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:repo-changed", repoPath: null }, "*");
		});

		function sendTuicMessage(iframe: HTMLIFrameElement, data: Record<string, unknown>) {
			const postMessageSpy = vi.fn();
			const fakeContentWindow = { postMessage: postMessageSpy };
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => fakeContentWindow,
			});
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			const event = new MessageEvent("message", { data });
			Object.defineProperty(event, "source", { get: () => fakeContentWindow });
			(handler as EventListener)(event);
		}

		it("writes the clipboard text on tuic:clipboard", async () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			sendTuicMessage(iframe, { type: "tuic:clipboard", text: "hello from plugin" });

			await vi.waitFor(() => {
				expect(mockWriteClipboard).toHaveBeenCalledWith("hello from plugin");
			});
			expect(mockAppLoggerWarn).not.toHaveBeenCalled();
		});

		it("logs a warning instead of throwing when tuic:clipboard write is denied", async () => {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			mockWriteClipboard.mockRejectedValue(err);
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			expect(() => sendTuicMessage(iframe, { type: "tuic:clipboard", text: "hello" })).not.toThrow();

			await vi.waitFor(() => {
				expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", expect.stringContaining("tuic:clipboard failed"));
			});
		});

		it("sdk-init is idempotent across repeated onLoad (e.g., in-iframe navigation)", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			const postMessageSpy = vi.fn();
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => ({ postMessage: postMessageSpy }),
			});

			iframe.dispatchEvent(new Event("load"));
			iframe.dispatchEvent(new Event("load"));
			iframe.dispatchEvent(new Event("load"));

			// 3 loads × 3 messages each (sdk-init + repo-changed + theme-changed) = 9
			expect(postMessageSpy).toHaveBeenCalledTimes(9);
			const sdkInitCalls = postMessageSpy.mock.calls.filter((call) => call[0]?.type === "tuic:sdk-init");
			expect(sdkInitCalls).toHaveLength(3);
			for (const call of sdkInitCalls) {
				expect(call[0]).toEqual({ type: "tuic:sdk-init", version: "1.0" });
			}
		});
	});

	describe("tuic:toast SDK message", () => {
		afterEach(() => {
			vi.restoreAllMocks();
		});

		/** Simulate the iframe posting a message to the host. The handler guards
		 *  on `event.source === iframeRef.contentWindow`, so this must use the
		 *  panel's own (real, happy-dom-provided) iframe window as the source —
		 *  a message with no matching source is silently dropped. */
		function dispatchFromIframe(iframe: HTMLIFrameElement, data: Record<string, unknown>) {
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			(handler as EventListener)(new MessageEvent("message", { data, source: iframe.contentWindow }));
		}

		it("forwards title/message/level/sound to toastsStore.add", () => {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const addSpy = vi.spyOn(toastsStore, "add");

			dispatchFromIframe(iframe, {
				type: "tuic:toast",
				title: "Done",
				message: "finished",
				level: "warn",
				sound: true,
			});

			expect(addSpy).toHaveBeenCalledWith("Done", "finished", "warn", true);
		});

		it("defaults message to '' and level to 'info' when omitted", () => {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const addSpy = vi.spyOn(toastsStore, "add");

			dispatchFromIframe(iframe, { type: "tuic:toast", title: "Hi" });

			expect(addSpy).toHaveBeenCalledWith("Hi", "", "info", false);
		});

		it("falls back to 'info' for an unrecognized level, rather than passing it through", () => {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const addSpy = vi.spyOn(toastsStore, "add");

			dispatchFromIframe(iframe, { type: "tuic:toast", title: "Hi", level: "critical" });

			expect(addSpy).toHaveBeenCalledWith("Hi", "", "info", false);
		});

		it("coerces a non-boolean sound value to false, not truthy-passthrough", () => {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const addSpy = vi.spyOn(toastsStore, "add");

			dispatchFromIframe(iframe, { type: "tuic:toast", title: "Hi", sound: "yes" });

			expect(addSpy).toHaveBeenCalledWith("Hi", "", "info", false);
		});

		it("logs a warning and never calls toastsStore.add when title is missing", () => {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const addSpy = vi.spyOn(toastsStore, "add");

			dispatchFromIframe(iframe, { type: "tuic:toast", message: "no title", sound: true });

			expect(addSpy).not.toHaveBeenCalled();
			expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", "tuic:toast missing title");
		});

		it("ignores a tuic:toast message whose source is not this panel's own iframe", () => {
			const tab = makeTab();
			render(() => <PluginPanel tab={tab} />);
			const addSpy = vi.spyOn(toastsStore, "add");

			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			// No `source` set — a spoofed/foreign message, not this panel's iframe.
			(handler as EventListener)(
				new MessageEvent("message", { data: { type: "tuic:toast", title: "Spoofed", sound: true } }),
			);

			expect(addSpy).not.toHaveBeenCalled();
		});
	});

	describe("other tuic:* SDK messages", () => {
		/** Same technique as the tuic:toast block: dispatch a real MessageEvent
		 *  at the registered window listener, sourced from the panel's own
		 *  (real, happy-dom-provided) iframe.contentWindow. */
		function dispatchFromIframe(iframe: HTMLIFrameElement, data: Record<string, unknown>) {
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			(handler as EventListener)(new MessageEvent("message", { data, source: iframe.contentWindow }));
		}

		beforeEach(() => {
			// Other describe blocks earlier in this file (and background effects
			// like repo/GitHub polling) also call the mocked invoke — clear its
			// call history so a "was fs_read_file called" assertion here can't be
			// polluted by unrelated calls made before this block ran.
			vi.mocked(tauriInvoke).mockClear();
		});

		afterEach(() => {
			vi.restoreAllMocks();
			repositoriesStore.setActive(null);
			repositoriesStore._testCancelPendingSave();
			for (const path of repositoriesStore.getPaths()) {
				repositoriesStore.remove(path);
			}
		});

		describe("tuic:open", () => {
			it("resolves a relative path against the active repo and opens a markdown tab", () => {
				repositoriesStore.setActive("/repo-x");
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(mdTabsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:open", path: "notes.md" });

				expect(addSpy).toHaveBeenCalledWith("/repo-x", "notes.md");
			});

			it("pins the tab when data.pinned is set", () => {
				repositoriesStore.setActive("/repo-x");
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const setPinnedSpy = vi.spyOn(mdTabsStore, "setPinned");

				dispatchFromIframe(iframe, { type: "tuic:open", path: "notes.md", pinned: true });

				expect(setPinnedSpy).toHaveBeenCalledWith(expect.any(String), true);
			});

			it("logs a warning and opens nothing when path is missing", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(mdTabsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:open" });

				expect(addSpy).not.toHaveBeenCalled();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", "tuic:open missing path");
			});

			it("logs a warning and opens nothing when the path cannot be resolved (no active repo)", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(mdTabsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:open", path: "notes.md" });

				expect(addSpy).not.toHaveBeenCalled();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith(
					"plugin",
					expect.stringContaining("tuic:open cannot resolve path"),
				);
			});
		});

		describe("tuic:edit", () => {
			it("resolves a relative path against the active repo and opens an editor tab at the given line", () => {
				repositoriesStore.setActive("/repo-x");
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(editorTabsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:edit", path: "src/index.ts", line: 42 });

				expect(addSpy).toHaveBeenCalledWith("/repo-x", "src/index.ts", 42);
			});

			it("passes undefined for line when omitted or zero", () => {
				repositoriesStore.setActive("/repo-x");
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(editorTabsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:edit", path: "src/index.ts" });

				expect(addSpy).toHaveBeenCalledWith("/repo-x", "src/index.ts", undefined);
			});

			it("logs a warning and opens nothing when path is missing", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(editorTabsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:edit" });

				expect(addSpy).not.toHaveBeenCalled();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", "tuic:edit missing path");
			});
		});

		describe("tuic:terminal", () => {
			it("opens a terminal at the given repoPath when the repo is registered", () => {
				repositoriesStore.add({ path: "/repo-y", displayName: "Repo Y" });
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(terminalsStore, "add").mockReturnValue("fake-term-id");

				dispatchFromIframe(iframe, { type: "tuic:terminal", repoPath: "/repo-y" });

				expect(addSpy).toHaveBeenCalledWith(expect.objectContaining({ cwd: "/repo-y", sessionId: null }));
			});

			it("logs a warning and opens nothing when repoPath is missing", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(terminalsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:terminal" });

				expect(addSpy).not.toHaveBeenCalled();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", "tuic:terminal missing repoPath");
			});

			it("logs a warning and opens nothing when repoPath is not a registered repo", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const addSpy = vi.spyOn(terminalsStore, "add");

				dispatchFromIframe(iframe, { type: "tuic:terminal", repoPath: "/not-registered" });

				expect(addSpy).not.toHaveBeenCalled();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith(
					"plugin",
					"tuic:terminal repo not in repo list: /not-registered",
				);
			});
		});

		describe("tuic:get-file", () => {
			it("resolves the path, reads the file over IPC, and posts the content back with the requestId", async () => {
				repositoriesStore.setActive("/repo-x");
				vi.mocked(tauriInvoke).mockResolvedValueOnce("file contents");
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const postMessageSpy = vi.spyOn(iframe.contentWindow as Window, "postMessage");

				dispatchFromIframe(iframe, { type: "tuic:get-file", path: "notes.md", requestId: "r1" });

				expect(tauriInvoke).toHaveBeenCalledWith("fs_read_file", { repoPath: "/repo-x", file: "notes.md" });
				await vi.waitFor(() => {
					expect(postMessageSpy).toHaveBeenCalledWith(
						{ type: "tuic:get-file-result", requestId: "r1", content: "file contents" },
						"*",
					);
				});
			});

			it("posts an error back when the IPC read rejects", async () => {
				repositoriesStore.setActive("/repo-x");
				vi.mocked(tauriInvoke).mockRejectedValueOnce(new Error("permission denied"));
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const postMessageSpy = vi.spyOn(iframe.contentWindow as Window, "postMessage");

				dispatchFromIframe(iframe, { type: "tuic:get-file", path: "notes.md", requestId: "r2" });

				await vi.waitFor(() => {
					expect(postMessageSpy).toHaveBeenCalledWith(
						{ type: "tuic:get-file-result", requestId: "r2", error: "Error: permission denied" },
						"*",
					);
				});
			});

			it("posts an unresolvable-path error synchronously, without ever calling IPC", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const postMessageSpy = vi.spyOn(iframe.contentWindow as Window, "postMessage");

				dispatchFromIframe(iframe, { type: "tuic:get-file", path: "notes.md", requestId: "r3" });

				expect(tauriInvoke).not.toHaveBeenCalledWith("fs_read_file", expect.anything());
				expect(postMessageSpy).toHaveBeenCalledWith(
					{ type: "tuic:get-file-result", requestId: "r3", error: "Cannot resolve path: notes.md" },
					"*",
				);
			});

			it("logs a warning and does nothing when path or requestId is missing", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;
				const postMessageSpy = vi.spyOn(iframe.contentWindow as Window, "postMessage");

				dispatchFromIframe(iframe, { type: "tuic:get-file", path: "notes.md" }); // no requestId

				expect(postMessageSpy).not.toHaveBeenCalled();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", "tuic:get-file missing path or requestId");
			});
		});

		describe("tuic:plugin-message", () => {
			it("forwards the payload to pluginRegistry.handlePanelMessage for this tab", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;

				dispatchFromIframe(iframe, { type: "tuic:plugin-message", payload: { foo: 1 } });

				expect(pluginRegistry.handlePanelMessage).toHaveBeenCalledWith("tab-1", { foo: 1 });
			});
		});

		describe("tuic:reload-request", () => {
			it("remounts the (srcdoc-mode) iframe", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const before = container.querySelector("iframe");

				dispatchFromIframe(before as HTMLIFrameElement, { type: "tuic:reload-request" });

				const after = container.querySelector("iframe");
				expect(after).not.toBe(before);
			});
		});

		describe("tuic:context-menu", () => {
			it("opens the reload context menu at the translated page coordinates", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;

				dispatchFromIframe(iframe, { type: "tuic:context-menu", x: 10, y: 20 });

				// happy-dom's getBoundingClientRect() is all-zero, so page coords == the
				// iframe-local ones sent — asserting via the menu's own positioning style
				// rather than reaching into the component's private `menu` handle.
				const menuEl = container.querySelector('[style*="left: 10px"][style*="top: 20px"]');
				expect(menuEl).not.toBeNull();
			});
		});

		describe("unrecognized tuic:* command", () => {
			it("logs a warning naming the unknown command instead of throwing", () => {
				const tab = makeTab();
				const { container } = render(() => <PluginPanel tab={tab} />);
				const iframe = container.querySelector("iframe") as HTMLIFrameElement;

				expect(() => dispatchFromIframe(iframe, { type: "tuic:not-a-real-command" })).not.toThrow();
				expect(mockAppLoggerWarn).toHaveBeenCalledWith("plugin", "Unknown tuic SDK command: tuic:not-a-real-command");
			});
		});
	});

	describe("system / non-tuic messages", () => {
		function dispatchFromIframe(iframe: HTMLIFrameElement, data: Record<string, unknown>) {
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			(handler as EventListener)(new MessageEvent("message", { data, source: iframe.contentWindow }));
		}

		it("close-panel for this tab's pluginId calls onClose", () => {
			const tab = makeTab();
			const onClose = vi.fn();
			const { container } = render(() => <PluginPanel tab={tab} onClose={onClose} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			dispatchFromIframe(iframe, { type: "close-panel", pluginId: "test-plugin" });

			expect(onClose).toHaveBeenCalledOnce();
		});

		it("close-panel for a DIFFERENT pluginId is ignored", () => {
			const tab = makeTab();
			const onClose = vi.fn();
			const { container } = render(() => <PluginPanel tab={tab} onClose={onClose} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			dispatchFromIframe(iframe, { type: "close-panel", pluginId: "some-other-plugin" });

			expect(onClose).not.toHaveBeenCalled();
		});

		it("a non-tuic, non-close-panel message routes to pluginRegistry.handlePanelMessage as-is", () => {
			const tab = makeTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			dispatchFromIframe(iframe, { type: "custom-plugin-event", value: 42 });

			expect(pluginRegistry.handlePanelMessage).toHaveBeenCalledWith("tab-1", {
				type: "custom-plugin-event",
				value: 42,
			});
		});
	});
});
