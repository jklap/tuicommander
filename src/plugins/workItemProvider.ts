/** Async port for data-only providers. Core owns rendering and host start-work execution. */
import type {
	WorkItemAction,
	WorkItemActionResult,
	WorkItemBoard,
	WorkItemCommentPage,
	WorkItemDetail,
	WorkItemFieldSchema,
	WorkItemPage,
	WorkItemQuery,
	WorkItemRef,
	WorkItemSavedFilter,
} from "../types/work-items.gen";

export interface WorkItemProvider {
	readonly providerId: string;
	readonly accountId: string;
	readonly name: string;
	readonly fields: readonly WorkItemFieldSchema[];
	readonly savedFilters: readonly WorkItemSavedFilter[];
	readonly boards: readonly WorkItemBoard[];
	list(query: WorkItemQuery, signal?: AbortSignal): Promise<WorkItemPage>;
	detail(reference: WorkItemRef, signal?: AbortSignal): Promise<WorkItemDetail>;
	comments(
		reference: WorkItemRef,
		cursor: string | null,
		limit: number,
		signal?: AbortSignal,
	): Promise<WorkItemCommentPage>;
	act(reference: WorkItemRef | null, action: WorkItemAction, signal?: AbortSignal): Promise<WorkItemActionResult>;
}
