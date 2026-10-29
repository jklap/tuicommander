import { afterEach, expect, it, vi } from "vitest";
import { IFRAME_EXTERNAL_LINK_SCRIPT } from "../../utils/iframeExternalLinks";

afterEach(() => vi.restoreAllMocks());

it("sends a clicked HTTPS link to the preview host and leaves a fragment inside the frame", () => {
	// A detached happy-dom document opens a new window even for a fragment.
	// Keep its native default action local while still asserting preventDefault below.
	vi.spyOn(window, "open").mockReturnValue(null);
	const doc = document.implementation.createHTMLDocument("Preview");
	const postMessage = vi.fn();
	const body = IFRAME_EXTERNAL_LINK_SCRIPT.slice(
		IFRAME_EXTERNAL_LINK_SCRIPT.indexOf(">") + 1,
		IFRAME_EXTERNAL_LINK_SCRIPT.lastIndexOf("</script>"),
	);
	new Function("document", "parent", body)(doc, { postMessage });
	const external = doc.createElement("a");
	external.href = "https://example.org/reference";
	doc.body.append(external);
	const click = new MouseEvent("click", { bubbles: true, cancelable: true });
	external.dispatchEvent(click);
	expect(click.defaultPrevented).toBe(true);
	expect(postMessage).toHaveBeenCalledWith(
		{ type: "tuic:preview-open-url", url: "https://example.org/reference" },
		"*",
	);
	postMessage.mockClear();
	const fragment = doc.createElement("a");
	fragment.href = "#section";
	doc.body.append(fragment);
	const fragmentClick = new MouseEvent("click", { bubbles: true, cancelable: true });
	fragment.dispatchEvent(fragmentClick);
	expect(fragmentClick.defaultPrevented).toBe(false);
	expect(postMessage).not.toHaveBeenCalled();
});
