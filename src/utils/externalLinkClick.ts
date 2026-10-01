import { handleOpenUrl } from "./openUrl";

/**
 * Document-level click handler that sends external http(s) links to the system
 * browser. Without it Tauri shows a "link could potentially be dangerous" dialog.
 *
 * It decides on the link text as written (`getAttribute`), never on `anchor.href`:
 * the resolved URL of a relative or `#` link is `http://<app origin>/...` whenever
 * the page is served over http (dev server, browser mode), which would open the
 * app's own origin in the browser for every local link.
 *
 * Markdown links carry `data-tuic-href` and are dispatched by ContentRenderer,
 * which also opens http(s) targets; handling them here would open them twice.
 */
export function handleExternalLinkClick(e: MouseEvent): void {
	const anchor = (e.target as HTMLElement).closest("a[href]");
	if (!anchor || anchor.hasAttribute("data-tuic-href")) return;
	const href = anchor.getAttribute("href");
	if (href && /^https?:\/\//i.test(href)) {
		e.preventDefault();
		handleOpenUrl(href);
	}
}
