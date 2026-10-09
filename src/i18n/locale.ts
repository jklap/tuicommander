import { createSignal } from "solid-js";

/**
 * The active UI locale, kept apart from the message catalogs on purpose.
 *
 * `stores/settings.ts` only needs to *set* the locale. Importing `setLocale`
 * from `./t` would drag the whole `en.json` catalog (~20 KB gzip) into any
 * bundle that reaches the store eagerly — mobile.html's initial load included,
 * whenever its import graph reaches the store again. `t.ts` re-uses this same signal,
 * so there is still exactly one locale.
 */
const [locale, setLocale] = createSignal("en");

export { locale, setLocale };
