import type {
	WorkItemAction,
	WorkItemActionResult,
	WorkItemBoard,
	WorkItemBoardCard,
	WorkItemBoardLane,
	WorkItemRef,
} from "../../../types/work-items.gen";

/** Authoritative provider/core projections. The UI does not infer workflow legality. */
export type BoardCard = WorkItemBoardCard;
export type BoardLane = WorkItemBoardLane;
export interface TasksBoardProps {
	scopeKey: string;
	board: WorkItemBoard;
	lanes: readonly BoardLane[];
	onMove(reference: WorkItemRef, action: WorkItemAction): Promise<WorkItemActionResult>;
	onUpdated?(result: WorkItemActionResult): void;
	onOpen?(reference: WorkItemRef): void;
}
export const cardKey = (reference: WorkItemRef): string =>
	JSON.stringify([reference.providerId, reference.accountId, reference.id]);
