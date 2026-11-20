/**
 * Claude Code asks "Is this a directory you created or one you trust?" for the
 * per-session `--add-dir` attachments directory. Trusting the attachments root once
 * silences it for every session. The backend only reports (read-only) whether each
 * Claude profile already trusts the root; these helpers render that and open the
 * terminal where the user answers Claude's own prompt. TUIC never writes the trust.
 */
import { appLogger } from "../stores/appLogger";
import { settingsStore } from "../stores/settings";
import { terminalsStore } from "../stores/terminals";
import { toastsStore } from "../stores/toasts";
import { rpc } from "../transport";
import { newTerminalShellFamily, scopedLaunchCommand } from "./agentLaunchCommand";
import { assignTabToActiveGroup } from "./paneTabAssign";
import { randomId } from "./randomId";

export interface AttachmentTrustProfile {
	/** `CLAUDE_CONFIG_DIR` the profile needs; null for the default profile. */
	config_dir: string | null;
	config_file: string;
	trusted: boolean;
}

export interface AttachmentTrustStatus {
	root: string;
	/** Empty when the claude agent is not installed. */
	profiles: AttachmentTrustProfile[];
}

export function getAttachmentTrustStatus(): Promise<AttachmentTrustStatus> {
	return rpc<AttachmentTrustStatus>("get_attachment_trust_status");
}

/** The command the button types: plain `claude`, scoped to a non-default profile. */
export function attachmentTrustCommand(profile: AttachmentTrustProfile): string {
	return scopedLaunchCommand(
		"claude",
		profile.config_dir ? { CLAUDE_CONFIG_DIR: profile.config_dir } : undefined,
		newTerminalShellFamily(),
	);
}

/**
 * Open a terminal tab in the attachments root that runs `claude`. Claude's own
 * trust dialog then appears for that directory; the user answers it once and exits.
 * The tab is unattached (no repo/branch), like the repo picker's "open unattached".
 */
export async function openAttachmentTrustTerminal(
	status: AttachmentTrustStatus,
	profile: AttachmentTrustProfile,
): Promise<void> {
	try {
		if (!(await rpc<boolean>("can_spawn_session"))) {
			appLogger.warn("terminal", "Cannot open the attachment-trust terminal: session limit reached");
			return;
		}
		const id = terminalsStore.add({
			sessionId: null,
			fontSize: settingsStore.state.defaultFontSize,
			name: "Trust attachments",
			cwd: status.root,
			awaitingInput: null,
			tuicSession: randomId(""),
		});
		// Not part of the create payload; typed at the terminal's first idle prompt.
		terminalsStore.update(id, { pendingInitCommand: attachmentTrustCommand(profile) });
		assignTabToActiveGroup(id, "terminal");
		terminalsStore.setActive(id);
		requestAnimationFrame(() => terminalsStore.get(id)?.ref?.focus());
	} catch (err) {
		appLogger.error("terminal", "Failed to open the attachment-trust terminal", err);
	}
}

let warnedThisLaunch = false;

/** One toast per launch when a Claude profile does not trust the attachments root. */
export async function warnIfAttachmentsUntrusted(): Promise<void> {
	if (warnedThisLaunch) return;
	try {
		const status = await getAttachmentTrustStatus();
		const untrusted = status.profiles.find((profile) => !profile.trusted);
		if (!untrusted) return;
		warnedThisLaunch = true;
		toastsStore.add(
			"Claude asks about TUIC's attachments folder",
			"Every Claude session TUIC starts is granted read access to its own uploads folder, and Claude asks you to trust it each time. Trust the parent folder once and the question stops. Claude can still read only the folder it was given.",
			"warn",
			false,
			{
				label: "Trust now",
				onClick: () => {
					void openAttachmentTrustTerminal(status, untrusted);
				},
			},
		);
	} catch (err) {
		appLogger.warn("app", "Could not check Claude's trust of the attachments folder", err);
	}
}
