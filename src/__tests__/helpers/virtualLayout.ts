/**
 * happy-dom has no real layout engine — every element reports `offsetHeight
 * === 0`. `@tanstack/solid-virtual`'s range calculation reads the scroll
 * container's `offsetHeight` (via `getRect`, `observeElementRect`) and each
 * row's `offsetHeight` (via `measureElement`, called synchronously from the
 * row's `ref`), so with the real value being 0 the virtualizer renders
 * either nothing or a single item — never enough to exercise reuse-by-index
 * bugs across several rows.
 *
 * This stubs `HTMLElement.prototype.offsetHeight` so the scroll container
 * (identified by the shared `container` class from
 * `src/components/shared/diffFileList.module.css` — `non-scoped` CSS module
 * class names, so this literal string is the real DOM class, not a hash)
 * reports a large height and every other element reports a small, fixed row
 * height. With a big container and a handful of short rows, every row in a
 * small test list ends up inside the visible range without needing to fake
 * scroll position or ResizeObserver callbacks — the initial synchronous
 * read in `observeElementRect`/`measureElement` is enough.
 *
 * Call `installVirtualLayout()` in `beforeEach` and the returned function (or
 * `uninstallVirtualLayout()`) in `afterEach` — this repo's other test helpers
 * (`helpers/store.ts`) don't self-register hooks, so this one doesn't either.
 */

const ROW_HEIGHT = 300;
const CONTAINER_HEIGHT = 20000;

let originalDescriptor: PropertyDescriptor | undefined;
let installed = false;

export function installVirtualLayout(options: { rowHeight?: number; containerHeight?: number } = {}): () => void {
	const rowHeight = options.rowHeight ?? ROW_HEIGHT;
	const containerHeight = options.containerHeight ?? CONTAINER_HEIGHT;

	if (!installed) {
		originalDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
		installed = true;
	}

	Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
		configurable: true,
		get() {
			const el = this as HTMLElement;
			return el.classList?.contains("container") ? containerHeight : rowHeight;
		},
	});

	return uninstallVirtualLayout;
}

export function uninstallVirtualLayout(): void {
	if (!installed) return;
	if (originalDescriptor) {
		Object.defineProperty(HTMLElement.prototype, "offsetHeight", originalDescriptor);
	}
	installed = false;
	originalDescriptor = undefined;
}
