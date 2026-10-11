export const HEARTBEAT_MS = 5000;
// Persist the acknowledged sequence across MV3 worker restarts.
export async function heartbeatReply(reply, storage, tabs, reviewedTab) {
  if (!reply.armed) return;
  const {reloadSeq = 0, reviewedTab: savedTab} = await storage.get(['reloadSeq', 'reviewedTab']);
  if (!Number.isSafeInteger(reply.reloadSeq)) return;
  // A backend restart restarts the counter below the saved acknowledgement: rebase, no reload.
  if (reply.reloadSeq < reloadSeq) await storage.set({reloadSeq: reply.reloadSeq});
  if (reply.reloadSeq <= reloadSeq) return;
  const id = reviewedTab ?? savedTab ?? (await tabs.query({active: true, lastFocusedWindow: true}))[0]?.id;
  if (!id) return;
  await tabs.reload(id);
  await storage.set({reloadSeq: reply.reloadSeq});
}
