import type { AgentRunConfig } from "../agents";
import { isWindows } from "../platform";
import { settingsStore } from "../stores/settings";
import type { ShellFamily } from "./sendCommand";
import { escapePosixShellArg } from "./shell";

/** Prefix an agent launch command with its run-config env, quoted for the shell that runs it. */
export function scopedLaunchCommand(
	command: string,
	env: AgentRunConfig["env"] | undefined,
	shellFamily: ShellFamily,
): string {
	const windows = isWindows();
	const entries = Object.entries(env ?? {}).filter(([key]) => {
		const name = windows || shellFamily === "windows-native" ? key.toUpperCase() : key;
		return /^[A-Za-z_][A-Za-z0-9_]*$/.test(key) && name !== "TUIC_SESSION" && name !== "TUIC_PARENT";
	});
	if (!entries.length) return command;
	if (shellFamily !== "windows-native" && (shellFamily === "posix" || !windows)) {
		return `env ${entries.map(([key, value]) => `${key}=${escapePosixShellArg(value)}`).join(" ")} ${command}`;
	}
	// EncodedCommand is accepted by both cmd and PowerShell and keeps values out of
	// the parent shell's parser and history. The child exits when the agent exits.
	const psQuote = (value: string) => `'${value.replaceAll("'", "''")}'`;
	const script = [
		...entries.map(([key, value]) => `$env:${key} = ${psQuote(value)}`),
		`Invoke-Expression ${psQuote(command)}`,
	].join("; ");
	let bytes = "";
	for (let index = 0; index < script.length; index++) {
		const codeUnit = script.charCodeAt(index);
		bytes += String.fromCharCode(codeUnit & 0xff, codeUnit >> 8);
	}
	return `powershell.exe -NoProfile -EncodedCommand ${btoa(bytes)}`;
}

/** Shell family of a terminal that is not created yet. */
export function newTerminalShellFamily(): ShellFamily {
	if (!isWindows()) return "posix";
	const shell = settingsStore.state.shell?.toLowerCase() ?? "";
	return /(?:^|[\\/])(?:bash|zsh|sh|fish|dash|ksh|ash|tcsh|csh|mksh|wsl)(?:\.exe)?(?:\s|$)/.test(shell)
		? "posix"
		: "windows-native";
}
