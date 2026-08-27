/**
 * Build a canonical (8-4-4-4-12) v4 UUID string from CSPRNG bytes. Used as the
 * fallback when `crypto.randomUUID` is unavailable, because several backend
 * validators (e.g. `tuic_session`'s prompt-injection guard, `RemoteConnection`
 * ids) hard-require UUID shape and reject anything else.
 */
function uuidFromRandomValues(getRandomValues: Crypto["getRandomValues"]): string {
	const bytes = new Uint8Array(16);
	getRandomValues.call(globalThis.crypto, bytes);
	bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
	bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 10
	const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * A random client-side id that survives a non-secure context.
 *
 * `crypto.randomUUID` is only defined in a secure context. TUIC is reached over
 * https tunnels but also over plain http on a LAN address, so a bare call
 * throws for exactly the remote clients that need the id most. `crypto.getRandomValues`
 * has no such restriction, so it is used to build an equivalent UUID by hand —
 * some callers pass this id straight through to backend validators that
 * require UUID shape, so the fallback must produce one too.
 *
 * As a last resort, for the exotic case where even `getRandomValues` is
 * missing, a non-UUID id is used. It is not globally unique, only distinct
 * among the live clients of one backend, which is all callers that tolerate
 * this path need.
 *
 * `prefix` keeps ids readable in logs. It must not contain `/` — the backend
 * qualifies watcher ids with that separator.
 */
export function randomId(prefix: string): string {
	const crypto = globalThis.crypto;
	const id =
		crypto?.randomUUID?.() ??
		(crypto?.getRandomValues ? uuidFromRandomValues(crypto.getRandomValues) : undefined) ??
		`${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
	return `${prefix}${id}`;
}
