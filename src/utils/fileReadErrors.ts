import { t } from "../i18n";

/** True when an HTTP-transport read was refused by the repo-root/allow-list gate
 *  (`mcp_http::fs_routes::access_denied`). HTTP-TRANSPORT ONLY: native Tauri's
 *  `read_external_file`/`read_editor_file_external` IPC commands have no such gate,
 *  so this can never fire on the desktop app — only for browser/remote/PWA clients.
 *  Matched on the 403 + "Access denied" pair (not the full sentence) so a reworded
 *  backend message doesn't silently regress this back to the raw-RPC path. Must NOT
 *  match a generic OS "permission denied" error. */
export function isAccessDeniedError(msg: string): boolean {
	return msg.includes("403") && msg.includes("Access denied");
}

export function accessDeniedMessage(): string {
	return t(
		"markdownTab.accessDenied",
		"This file is outside your registered repositories and allowed directories, so it can't be read in browser/remote mode. Add its folder under Settings → Remote Access → File Access → Additional Readable Directories.",
	);
}
