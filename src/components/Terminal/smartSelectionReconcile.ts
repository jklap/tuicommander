import { canonicalAction, type SmartSelectionRule } from "./smartSelectionTypes";

/**
 * Everything a user can edit about a rule, as one comparable string. `enabled` is left out on
 * purpose: switching a rule on or off is a preference, not an edit of the rule, so a disabled
 * built-in still counts as untouched (and keeps its switch when a release updates it).
 * `builtInHash` is bookkeeping and never part of the content.
 */
export function ruleContentKey(rule: SmartSelectionRule): string {
	return JSON.stringify([
		rule.id,
		rule.name,
		rule.regex,
		rule.precision,
		// The per-action options are appended ONLY when set, never as a placeholder: this key is
		// hashed into every saved rule's baseline, so a rule that does not use an option must
		// produce exactly the key it did before the option existed.
		// Through `canonicalAction`: the older `run_command` IS `send_text` + when_safe, so a saved
		// rule and the shipped one that spell the same behaviour differently hash as equal.
		rule.actions
			.map(canonicalAction)
			.map((a) => [
				a.kind,
				a.title,
				a.parameter,
				a.isDefault,
				...(a.sendReturn ? ["sendReturn", a.sendReturn] : []),
				...(a.runIn ? ["runIn", a.runIn] : []),
			]),
	]);
}

/** FNV-1a (32-bit) of a rule's content, as 8 hex chars. Stable across platforms and releases,
 *  small enough to live on every saved rule; not a security boundary. */
export function ruleContentHash(rule: SmartSelectionRule): string {
	let hash = 0x811c9dc5;
	const key = ruleContentKey(rule);
	for (let i = 0; i < key.length; i++) {
		hash ^= key.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193) >>> 0;
	}
	return hash.toString(16).padStart(8, "0");
}

export interface ReconcileResult {
	rules: SmartSelectionRule[];
	/** Names of built-in rules the user never edited that were replaced with the new shipped version. */
	updated: string[];
	/** Names of built-in rules the user HAS edited whose shipped version changed: the new version was
	 *  added next to them as a disabled copy, and theirs was left exactly as it was. */
	conflicts: string[];
	/** True when `rules` differs from the input in any way, including silent bookkeeping, so the
	 *  caller knows to persist. Only `updated`/`conflicts` are worth telling the user about. */
	changed: boolean;
}

/**
 * Bring a user's saved rules in line with the shipped set after a release changed a built-in.
 *
 * Saving any rule writes the whole shipped set into the user's config (see
 * `resolveSmartSelectionRules`), so without this a changed shipped rule never reaches anyone who
 * has customized anything. Each saved copy of a built-in carries `builtInHash`, the hash of the
 * shipped definition it was last reconciled against, which makes "has the user edited this?" a
 * comparison of two hashes and needs no history of old releases:
 *
 *  - up to date (`builtInHash` equals the shipped hash): nothing to do;
 *  - no `builtInHash` (saved before this existed): stamp the CURRENT shipped hash and change
 *    nothing else. We cannot know what it was reconciled against, so an edited copy stays "edited"
 *    and an untouched one stays "untouched"; a later release then behaves correctly for both;
 *  - shipped changed and the user never edited their copy: replace it in place with the new
 *    version (keeping its on/off switch);
 *  - shipped changed and the user edited their copy to exactly the new version: just record it;
 *  - shipped changed and the user edited their copy otherwise: keep theirs untouched and add the new version
 *    right after it as a disabled copy named "<name> (updated)", so nothing they wrote is lost and
 *    nothing new is active until they look at it.
 *
 * A saved list that is empty means "use the shipped set live", so there is nothing to reconcile.
 * A shipped rule the user deleted stays deleted, and a rule that is no longer shipped is left alone.
 *
 * DEFERRED (2026-11-16) — a rule that is NEW in a release (an id the saved list has never held) is
 * not added to an already-customized list. Telling "never seen" from "deleted on purpose" needs a
 * record of which shipped ids the user has been offered (e.g. a `seen_builtin_ids` list in the
 * config); without it a deleted rule would come back on every launch. Worth doing the first time a
 * release actually ships a new built-in rule.
 */
export function reconcileBuiltInRules(saved: SmartSelectionRule[], shipped: SmartSelectionRule[]): ReconcileResult {
	if (saved.length === 0) return { rules: saved, updated: [], conflicts: [], changed: false };

	const shippedById = new Map(shipped.map((rule) => [rule.id, rule]));
	const takenIds = new Set(saved.map((rule) => rule.id));
	const updated: string[] = [];
	const conflicts: string[] = [];
	const rules: SmartSelectionRule[] = [];
	let changed = false;

	for (const rule of saved) {
		const current = shippedById.get(rule.id);
		if (!current) {
			rules.push(rule);
			continue;
		}
		const shippedHash = ruleContentHash(current);
		const baseline = rule.builtInHash;

		if (baseline === shippedHash) {
			rules.push(rule);
		} else if (baseline === undefined) {
			rules.push({ ...rule, builtInHash: shippedHash });
			changed = true;
		} else if (ruleContentHash(rule) === baseline) {
			rules.push({ ...current, enabled: rule.enabled, builtInHash: shippedHash });
			updated.push(current.name);
			changed = true;
		} else if (ruleContentHash(rule) === shippedHash) {
			// Edited by hand into exactly the new shipped version: nothing to add, just record it.
			rules.push({ ...rule, builtInHash: shippedHash });
			changed = true;
		} else {
			rules.push({ ...rule, builtInHash: shippedHash });
			changed = true;
			const copyId = `${current.id}-update-${shippedHash}`;
			if (!takenIds.has(copyId)) {
				takenIds.add(copyId);
				rules.push({ ...current, id: copyId, name: `${current.name} (updated)`, enabled: false });
				conflicts.push(current.name);
			}
		}
	}

	return { rules: changed ? rules : saved, updated, conflicts, changed };
}
