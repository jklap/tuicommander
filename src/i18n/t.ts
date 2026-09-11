import { createSignal } from "solid-js";

const [locale, setLocale] = createSignal("en");

export { locale, setLocale };

export function t(_key: string, fallback: string, params?: Record<string, string>): string {
	let str = fallback;
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			// String.replace treats "$&", "$1", "$$", etc. in the replacement string as
			// special patterns — escape "$" so an interpolated value (e.g. a branch name)
			// is always inserted literally, never reinterpreted.
			str = str.replace(new RegExp(`\\{${k}\\}`, "g"), v.replace(/\$/g, "$$$$"));
		}
	}
	return str;
}
