import { getRemoteBaseUrl, getRepoConnection, withRemoteToken } from "../transportRuntime";

/** Browser URL of a repository image, served by the authenticated `/fs/markdown-image` route. */
export function repoImageUrl(repoPath: string, file: string): string {
	const connectionId = getRepoConnection(repoPath);
	const base = connectionId ? getRemoteBaseUrl(connectionId) : undefined;
	const url = new URL("/fs/markdown-image", base ?? window.location.origin);
	url.searchParams.set("repoPath", repoPath);
	url.searchParams.set("file", file);
	return withRemoteToken(url.toString(), connectionId);
}
