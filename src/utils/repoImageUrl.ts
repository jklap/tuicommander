import { convertFileSrc } from "@tauri-apps/api/core";
import { locateFile } from "../stores/repositories";
import { isTauri } from "../transport";
import { getRemoteBaseUrl, getRepoConnection, withRemoteToken } from "../transportRuntime";
import { isAbsolutePath, joinPath } from "./pathUtils";

/** Browser URL of a repository image, served by the authenticated `/fs/markdown-image` route. */
export function repoImageUrl(repoPath: string, file: string, ownerRepoPath = repoPath): string {
	const connectionId = getRepoConnection(ownerRepoPath);
	const base = connectionId ? getRemoteBaseUrl(connectionId) : undefined;
	const url = new URL("/fs/markdown-image", base ?? window.location.origin);
	url.searchParams.set("repoPath", repoPath);
	url.searchParams.set("file", file);
	return withRemoteToken(url.toString(), connectionId);
}

/** Resolve browser assets against registered roots, including linked worktrees. */
export function browserLocalImageUrl(path: string): string | null {
	const location = locateFile(path);
	return location.fsRoot ? repoImageUrl(location.fsRoot, location.filePath, location.repoPath) : null;
}

/** Keep unavailable local images visible without issuing a doomed browser request. */
export function rewriteLocalImages(html: string, baseDir?: string, imageSrc?: (path: string) => string): string {
	if (isTauri() && !baseDir && !imageSrc) return html;
	return html.replace(
		/<img\b[^>]*\ssrc=(?:"([^"]*)"|'([^']*)'|([^\s>]+))[^>]*>/gi,
		(tag, doubleQuoted: string | undefined, singleQuoted: string | undefined, unquoted: string | undefined) => {
			const path = doubleQuoted ?? singleQuoted ?? unquoted ?? "";
			if (/^(https?:\/\/|\/\/|data:|asset:\/\/)/i.test(path)) return tag;
			const local = isAbsolutePath(path) ? path : baseDir ? joinPath(baseDir, path) : path;
			const src = imageSrc ? imageSrc(path) : isTauri() ? convertFileSrc(local) : browserLocalImageUrl(local);
			if (!src || (!isTauri() && !/^(https?:\/\/|\/\/|data:)/i.test(src))) {
				return '<span role="img" aria-label="Image unavailable">Image unavailable: outside open repositories.</span>';
			}
			return tag.replace(/\ssrc=(?:"[^"]*"|'[^']*'|[^\s>]+)/i, () => ` src="${src}"`);
		},
	);
}
