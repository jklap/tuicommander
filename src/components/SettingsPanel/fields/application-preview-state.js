import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
const [state, setState] = createStore({
  language: "en",
  showLastPrompt: true,
  closeBehavior: "ask",
  trayIcon: "background_only",
  hideDockIcon: true,
  confirmBeforeQuit: true,
  confirmBeforeClosingTab: true,
  preventSleepWhenBusy: false,
  standbyTimeoutMinutes: 5,
  indexStrategy: "active_and_switch",
  autoUpdateEnabled: true,
  deferUpdateRelaunch: true,
  eagerResumeAfterUpdate: true,
  updateChannel: "stable",
  egoExecutable: "/workspace/tools/ego",
  egoProfile: "coordinator",
  aiChatWorkspace: "/workspace/long-project-name/application-development",
  ide: "vscode",
  customLaunchers: [],
  experimentalFeaturesEnabled: false,
  splitTabMode: "separate",
  tabOrderingMode: "grouped-by-type",
  tabCyclingAllTypes: false,
  tabTreeEnabled: true,
  maxTabNameLength: 25,
  chatMuteIntermediateText: true
});
export const settingsStore = new Proxy({ state, isAcpConfigured: () => !!state.egoExecutable }, { get(target, key) {
  if (key in target) return target[key];
  return (value) => {
    const field = key.slice(3);
    setState(field[0].toLowerCase() + field.slice(1), value);
  };
} });
export const IDE_NAMES = { vscode: "VS Code", cursor: "Cursor", none: "None" };
export const updaterStore = { state: { checking: false, downloading: false, available: false, version: null, error: null, unsupported: "Illustrative fixture: update status has not been checked." }, checkForUpdate: async () => {
} };
const [repoState, setRepoState] = createStore({ groups: { work: { id: "work", name: "Work", color: "#4a9eff", collapsed: false, repoOrder: [] }, personal: { id: "personal", name: "Personal projects with a longer group name", color: "#a578d4", collapsed: false, repoOrder: [] } }, groupOrder: ["work", "personal"] });
export const repositoriesStore = { state: repoState, renameGroup: (id, name) => {
  if (Object.values(repoState.groups).some((g) => g.id !== id && g.name === name)) return false;
  setRepoState("groups", id, "name", name);
  return true;
}, setGroupColor: (id, color) => setRepoState("groups", id, "color", color), deleteGroup: (id) => setRepoState("groupOrder", (list) => list.filter((item) => item !== id)), createGroup: () => {
} };
const [notificationState, setNotificationState] = createStore({ isAvailable: true, autoToastCards: true, config: { enabled: true, volume: 0.5, audio_device: null, sounds: { question: true, error: true, completion: true, warning: true, info: false, attention: true }, silence_remote_completions: true, toasts_in_bell: true, pr_native_notifications: true } });
export const notificationsStore = { state: notificationState, setVolume: (v) => setNotificationState("config", "volume", v), setEnabled: (v) => setNotificationState("config", "enabled", v), setSoundEnabled: (k, v) => setNotificationState("config", "sounds", k, v), setAudioDevice: (v) => setNotificationState("config", "audio_device", v), setAutoToastCards: (v) => setNotificationState("autoToastCards", v), setSilenceRemoteCompletions: (v) => setNotificationState("config", "silence_remote_completions", v), setToastsInBell: (v) => setNotificationState("config", "toasts_in_bell", v), setPrNativeNotifications: (v) => setNotificationState("config", "pr_native_notifications", v), testSound: () => {
}, reset: () => {
} };
const [enabled, setEnabled] = createSignal(true);
export const transcriptPreferences = { enabled, setEnabled };
export const tasksNotifications = { enabled: () => false, setEnabled: () => {
} };
export const uiStore = { resetLayout: () => {
} };
export const appLogger = { error: () => {
}, warn: () => {
}, info: () => {
}, debug: () => {
} };
export const isTauri = () => true;
export const isMacOS = () => true;
export const openDialog = async () => null;
export const invoke = async (cmd) => ({ get_home_directory: "/workspace", get_cli_status: { installed: true, path: "/usr/local/bin/tuic", version_match: true, auto_updatable: true }, mdkb_status: { available: true, connected: false, binaryPath: "/usr/local/bin/mdkb", version: "fixture" }, list_audio_output_devices: [{ name: "Illustrative speakers", is_default: true }] })[cmd];
export const ExpertSetting = (props) => props.children;

export const rpc=async()=>null;
export const owningConnectionFor=()=>undefined;
export const listen=async()=>()=>{};
export const UiLegend=()=>"Visual reference: busy, idle, attention and error states (isolated fixture).";
