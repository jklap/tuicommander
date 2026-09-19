/**
 * A `ClipboardItem` test double that records what it was constructed with,
 * instead of relying on happy-dom's own (partial) `ClipboardItem` semantics.
 * Install via `globalThis.ClipboardItem = FakeClipboardItem` (save and restore
 * the original in a try/finally) wherever a test needs to inspect the exact
 * promise `writeClipboardAsync` (utils/clipboard.ts) handed it, independent of
 * whatever a real implementation does with that promise internally.
 */
export class FakeClipboardItem {
	types: string[];
	constructor(readonly init: Record<string, PromiseLike<Blob>>) {
		this.types = Object.keys(init);
	}
}
