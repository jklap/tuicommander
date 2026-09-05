import { createSignal } from "solid-js";
import en from "./en.json";

const [locale, setLocale] = createSignal("en");

/**
 * Message catalogs by locale code. `en.json` is generated from the call sites,
 * so an English lookup returns the same text the fallback already carries.
 * A locale with no catalog renders the fallbacks.
 */
const catalogs: Record<string, Record<string, string>> = { en };

export { locale, setLocale };

export function t(key: string, fallback: string, params?: Record<string, string>): string {
	// Reading locale() here is what makes every t() call site re-render on a
	// language change.
	const message = catalogs[locale()]?.[key];
	let str = typeof message === "string" ? message : fallback;
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			str = str.replace(new RegExp(`\\{${k}\\}`, "g"), v);
		}
	}
	return str;
}
