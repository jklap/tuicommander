/** iPad Safari requests the desktop shell with a Mac user agent. */
export function tabletAppDestination(
	userAgent: string,
	maxTouchPoints: number,
	pathname: string,
	search: string,
	hash: string,
	native: boolean,
): string | null {
	if (native || pathname !== "/" || /^#\/secret-form(?:\?|$)/.test(hash)) return null;
	if (!/iPad|Macintosh/.test(userAgent) || maxTouchPoints <= 1) return null;
	return `/mobile${search}${hash}`;
}
