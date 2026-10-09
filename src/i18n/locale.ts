import { createSignal } from "solid-js";

/**
 * The active UI locale, kept apart from the message catalogs on purpose.
 *
 * `stores/settings.ts` only needs to *set* the locale, and the mobile entry
 * reaches that store eagerly (MobileApp -> SessionCard -> activitySnapshot ->
 * stores/terminals -> stores/settings). Importing `setLocale` from `./t` would
 * drag the whole `en.json` catalog (~20 KB gzip) into mobile.html's initial
 * load, which the bundle budget cannot afford. `t.ts` re-uses this same signal,
 * so there is still exactly one locale.
 */
const [locale, setLocale] = createSignal("en");

export { locale, setLocale };
