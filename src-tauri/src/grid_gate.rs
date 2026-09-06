//! Flow control for grid-frame delivery, on both transports.
//!
//! A grid frame is usually a DELTA — only the rows alacritty marked dirty — so
//! the client's row map is a stateful reassembly of everything it has received.
//! Dropping one frame silently strands whatever rows it carried. Both halves of
//! this module exist because a drop mechanism was applied to that stream anyway:
//! the desktop gate could not tell a fresh ack from a late one, and the WS
//! channel skipped intermediate frames on purpose.
//!
//! The frame ticker may only send while the frontend has caught up. That was a
//! bare `AtomicBool` cleared by every ack, and it had no way to tell one frame's
//! ack from another's: after the ticker gave a frame up for lost (500 ms) and
//! sent the next one, the late ack for the abandoned frame reopened the gate and
//! a third frame went out at once — a burst delivered at exactly the moment the
//! frontend was proven to be behind.
//!
//! Counting fixes that without touching the wire format. Rust counts frames it
//! sends; the frontend counts frames it receives and echoes that total on each
//! ack. The gate is open only when the echoed total has caught up with what was
//! sent, so an ack for an abandoned frame is a number that is already in the
//! past and changes nothing.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Hands out a fresh id to every subscription, process-wide.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// Hands out the order a frame was cut from a grid in, process-wide.
///
/// Global rather than per session so that a session whose watch channel is
/// replaced — every spawn path installs a fresh one — cannot start behind the
/// order that channel already recorded and have every frame rejected as stale.
static NEXT_FRAME_ORDER: AtomicU64 = AtomicU64::new(1);

/// The bytes of one grid frame plus the order it was cut from the grid in.
///
/// Five producers serialize under the vt lock and hand the bytes to
/// `send_grid_frame` *after* releasing it, so the order frames reach a transport
/// is not the order they were cut: a resize can serialize a full frame, lose the
/// race to a delta cut later, and land last — repainting the client with the
/// older screen, which is the "blank after zoom" the resize flush exists to
/// prevent. The order is stamped inside the critical section that produced the
/// bytes, because that is the only place it exists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridFrame {
    pub(crate) order: u64,
    pub(crate) bytes: Vec<u8>,
}

impl GridFrame {
    /// Wrap freshly serialized bytes, claiming the next order for them. Call
    /// while the vt lock that produced `bytes` is still held.
    pub(crate) fn cut(bytes: Vec<u8>) -> Self {
        Self {
            order: NEXT_FRAME_ORDER.fetch_add(1, Ordering::Relaxed),
            bytes,
        }
    }

    /// Nothing changed, so there is nothing to send.
    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Where a frame stands against the newest one already handed to a transport.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FrameOrder {
    /// Nothing newer has been delivered; this frame may go out.
    Newest,
    /// A frame cut *after* this one was already delivered. This one carries rows
    /// that newer frame does not — the damage behind them was consumed when it
    /// was serialized — so it can neither be sent nor silently dropped.
    Stale,
}

/// Frame-delivery gate for one session. Cheap: two relaxed atomics, no lock.
#[derive(Debug)]
pub(crate) struct GridGate {
    /// Identifies the subscription this gate belongs to. Acks name it, so a
    /// message from the previous subscription cannot be mistaken for a fresh one.
    epoch: u64,
    /// Frames handed to the channel since this gate was installed.
    sent: AtomicU64,
    /// Frames the frontend has reported receiving, clamped to `sent`.
    acked: AtomicU64,
    /// Set when a frame was withheld from this channel because the gate was
    /// closed while the WebSocket transport kept receiving. See
    /// [`Self::note_missed`].
    missed: AtomicBool,
}

impl GridGate {
    pub(crate) fn new() -> Self {
        Self {
            epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
            sent: AtomicU64::new(0),
            acked: AtomicU64::new(0),
            missed: AtomicBool::new(false),
        }
    }

    /// The id of the subscription this gate serves.
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// A frame just went on the wire.
    pub(crate) fn mark_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    /// The frontend reports `received` frames delivered in total, for the
    /// subscription `epoch` names.
    ///
    /// An ack from a previous subscription is dropped outright: the frontend
    /// restarts its count at zero on resubscribe, so an old ack still in flight
    /// carries a number that means nothing here — and clamping it to what this
    /// gate has sent would credit exactly the frame still waiting to be painted.
    /// Within the right epoch the value is monotonic and clamped, so a reordered
    /// or duplicated ack can neither rewind the gate nor open it for frames that
    /// were never sent.
    pub(crate) fn ack(&self, epoch: u64, received: u64) {
        if epoch != self.epoch {
            return;
        }
        let sent = self.sent.load(Ordering::Relaxed);
        let bounded = received.min(sent);
        self.acked.fetch_max(bounded, Ordering::Relaxed);
    }

    /// True when no frame is outstanding, i.e. the ticker may send.
    pub(crate) fn is_open(&self) -> bool {
        self.acked.load(Ordering::Relaxed) >= self.sent.load(Ordering::Relaxed)
    }

    /// Give up on every outstanding frame (the frontend missed its deadline).
    ///
    /// This is what makes a late ack harmless: the gate moves forward to what was
    /// actually sent, so the ack that arrives afterwards is stale by definition.
    pub(crate) fn abandon(&self) {
        self.acked
            .fetch_max(self.sent.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    /// A frame was serialized and delivered to the WebSocket subscribers while
    /// this gate was closed, so it never reached this channel.
    ///
    /// The gate used to stop the frame ticker outright, which held the damage on
    /// the vt until the WebView caught up — lossless for the desktop, and it
    /// starved every browser subscriber on the other transport. They keep
    /// receiving now, which means the rows in those frames left the shared damage
    /// without reaching this channel. A delta on top of a row map missing them
    /// would leave stale rows with no error, so the debt is recorded here and
    /// paid with a full frame once the gate reopens.
    pub(crate) fn note_missed(&self) {
        self.missed.store(true, Ordering::Relaxed);
    }

    /// Take the debt [`Self::note_missed`] recorded, if any. True exactly once
    /// per stall: the caller owes this subscription one full frame.
    pub(crate) fn take_missed(&self) -> bool {
        self.missed.swap(false, Ordering::Relaxed)
    }

    /// Frames sent and not yet reported as received — for diagnostics.
    pub(crate) fn outstanding(&self) -> u64 {
        self.sent
            .load(Ordering::Relaxed)
            .saturating_sub(self.acked.load(Ordering::Relaxed))
    }
}

/// One frame published to the WebSocket grid clients, tagged with a per-session
/// sequence number.
///
/// The channel underneath is a `watch`, which keeps only the newest value: a
/// client that cannot keep up skips frames. That is right for a full snapshot and
/// wrong for a delta, so the sequence lets the reader see the skip and repair it
/// with a full frame instead of rendering a row map with holes in it. The number
/// never leaves Rust — the wire format is untouched.
#[derive(Clone, Debug, Default)]
pub(crate) struct GridWatchFrame {
    pub(crate) seq: u64,
    /// Highest [`GridFrame::order`] handed to any of this session's transports.
    /// Distinct from `seq`, which counts what this channel published: a frame
    /// can be delivered to the desktop and to nobody here, and a reader that
    /// treated the two as one number would see a gap where none exists.
    pub(crate) order: u64,
    pub(crate) frame: Vec<u8>,
}

pub(crate) type GridWatchTx = tokio::sync::watch::Sender<GridWatchFrame>;

/// A fresh grid watch channel, seeded with the empty frame (`seq` 0).
pub(crate) fn new_grid_watch() -> GridWatchTx {
    tokio::sync::watch::channel(GridWatchFrame::default()).0
}

/// Claim `order` as the newest frame delivered for this session and, when
/// `frame` is `Some`, publish it to the watchers under the same sequence number.
///
/// One critical section on purpose. Claiming the order and publishing the bytes
/// separately reopens exactly the window this closes: two producers could claim
/// in the order they serialized and then publish in the other one.
///
/// `frame` is `None` when nobody is watching. The order is still claimed,
/// because the desktop channel and this watch are two transports for one grid
/// and one damage set, so they share one ordering.
pub(crate) fn claim_grid_frame(tx: &GridWatchTx, order: u64, frame: Option<Vec<u8>>) -> FrameOrder {
    let mut verdict = FrameOrder::Stale;
    tx.send_if_modified(|slot| {
        if order <= slot.order {
            return false;
        }
        slot.order = order;
        verdict = FrameOrder::Newest;
        match frame {
            Some(bytes) => {
                slot.seq += 1;
                slot.frame = bytes;
                true
            }
            // Nothing was published, so nothing changed for a reader: waking one
            // here would spend a frame's worth of work on the bytes it already has.
            None => false,
        }
    });
    verdict
}

/// One producer publishing a freshly cut frame: take the next order, then hand
/// the bytes to the watchers under it.
///
/// Test-only. Production code never cuts and publishes in one step — the order
/// has to be stamped inside the vt lock and the bytes travel out of it, which is
/// the whole reason [`GridFrame`] exists.
#[cfg(test)]
pub(crate) fn publish_grid_frame(tx: &GridWatchTx, bytes: Vec<u8>) -> FrameOrder {
    let frame = GridFrame::cut(bytes);
    claim_grid_frame(tx, frame.order, Some(frame.bytes))
}

/// Free the retained frame after the last grid client leaves.
///
/// A `watch` keeps its last value for the life of the channel, and the channel
/// lives as long as the session — so a browser that watched a 110 KB grid once
/// pinned those bytes until the PTY closed. Nothing needs them: a client that
/// reconnects is answered with a fresh full frame.
///
/// The sequence is deliberately left alone. It is what a reconnecting reader
/// compares against to tell a delta from a gap, and rewinding it would make the
/// next real frame look like a replay.
pub(crate) fn release_grid_frame(tx: &GridWatchTx) {
    tx.send_modify(|slot| slot.frame = Vec::new());
}

/// Did the watch channel drop frames between `last_seq` and `seq`?
///
/// Consecutive means nothing was lost. The same sequence twice means the reader
/// was woken without a new value, which loses nothing either. Anything else — a
/// skip forward, or a sequence that went backwards because the channel was
/// replaced under the reader — means at least one delta went missing and the
/// client's row map can no longer be trusted, so the reader must send a full
/// frame rather than the delta it just picked up.
pub(crate) fn watch_dropped_frames(last_seq: u64, seq: u64) -> bool {
    seq != last_seq && seq != last_seq + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Retained watch frame (604-cb45 F18) ---

    #[test]
    fn releasing_the_watch_frame_frees_the_bytes_but_keeps_the_sequence() {
        let tx = new_grid_watch();
        publish_grid_frame(&tx, vec![7u8; 64 * 1024]);
        let seq_before = tx.borrow().seq;

        // The last grid client left. Holding its final frame for the rest of the
        // session buys nothing — a reconnect is answered with a fresh full frame.
        release_grid_frame(&tx);

        assert!(
            tx.borrow().frame.is_empty(),
            "the retained frame must be freed"
        );
        assert_eq!(
            tx.borrow().seq,
            seq_before,
            "the sequence must not rewind — a reconnecting client compares against it"
        );
    }

    #[test]
    fn a_released_watch_still_publishes_afterwards() {
        let tx = new_grid_watch();
        publish_grid_frame(&tx, vec![1u8, 2, 3]);
        release_grid_frame(&tx);

        publish_grid_frame(&tx, vec![4u8, 5]);

        assert_eq!(tx.borrow().frame, vec![4u8, 5]);
        assert_eq!(
            tx.borrow().seq,
            2,
            "release must not consume a sequence number"
        );
    }

    #[test]
    fn a_fresh_gate_is_open() {
        let gate = GridGate::new();
        assert!(gate.is_open());
        assert_eq!(gate.outstanding(), 0);
    }

    #[test]
    fn sending_closes_the_gate_and_an_ack_for_that_frame_opens_it() {
        let gate = GridGate::new();
        gate.mark_sent();
        assert!(!gate.is_open());
        assert_eq!(gate.outstanding(), 1);

        gate.ack(gate.epoch(), 1);
        assert!(gate.is_open());
        assert_eq!(gate.outstanding(), 0);
    }

    /// The regression this type exists for. Old behaviour: force-reset → send
    /// frame 2 → late ack for frame 1 clears the flag → frame 3 goes out at once.
    #[test]
    fn a_late_ack_for_an_abandoned_frame_does_not_open_the_gate() {
        let gate = GridGate::new();
        gate.mark_sent(); // frame 1
        gate.abandon(); // ticker waited 500 ms and gave up
        assert!(gate.is_open(), "the ticker must be able to send frame 2");

        gate.mark_sent(); // frame 2
        assert!(!gate.is_open());

        gate.ack(gate.epoch(), 1); // the frontend finally reports frame 1
        assert!(
            !gate.is_open(),
            "frame 2 is still outstanding; opening here is the burst bug"
        );

        gate.ack(gate.epoch(), 2);
        assert!(gate.is_open());
    }

    /// A frontend that painted several frames in one turn reports the total, not
    /// one ack per frame.
    #[test]
    fn one_ack_can_clear_several_frames() {
        let gate = GridGate::new();
        for _ in 0..4 {
            gate.mark_sent();
        }
        assert_eq!(gate.outstanding(), 4);
        gate.ack(gate.epoch(), 4);
        assert!(gate.is_open());
    }

    #[test]
    fn an_ack_never_rewinds_the_gate() {
        let gate = GridGate::new();
        gate.mark_sent();
        gate.mark_sent();
        gate.ack(gate.epoch(), 2);
        gate.ack(gate.epoch(), 1); // reordered/duplicated ack
        assert!(gate.is_open());
    }

    /// The frontend resets its counter when it (re)subscribes and Rust installs a
    /// fresh gate; an ack already in flight from the old channel carries a huge
    /// number that must not open the new gate.
    #[test]
    fn an_ack_cannot_run_ahead_of_what_was_sent() {
        let gate = GridGate::new();
        gate.ack(gate.epoch(), 57);
        gate.mark_sent();
        assert!(
            !gate.is_open(),
            "a stale ack from a previous subscription must not credit a new frame"
        );
        gate.ack(gate.epoch(), 1);
        assert!(gate.is_open());
    }

    /// Clamping alone is not enough once the NEW gate has sent something: an ack
    /// from the previous subscription (already in flight when the frontend
    /// resubscribed) would clamp to that frame's number and credit it, releasing
    /// the next frame while the first is still unpainted. The epoch is what tells
    /// the two subscriptions apart.
    #[test]
    fn an_ack_from_a_previous_subscription_is_ignored() {
        let old = GridGate::new();
        let new = GridGate::new();
        assert_ne!(
            old.epoch(),
            new.epoch(),
            "each subscription gets its own id"
        );

        new.mark_sent();
        new.ack(old.epoch(), 57);
        assert!(
            !new.is_open(),
            "an ack addressed to the previous subscription credited this one"
        );

        new.ack(new.epoch(), 1);
        assert!(new.is_open());
    }

    #[test]
    fn epochs_keep_increasing() {
        let first = GridGate::new().epoch();
        let second = GridGate::new().epoch();
        let third = GridGate::new().epoch();
        assert!(first < second && second < third);
    }

    #[test]
    fn consecutive_watch_frames_are_not_a_drop() {
        assert!(!watch_dropped_frames(0, 1));
        assert!(!watch_dropped_frames(41, 42));
    }

    #[test]
    fn a_skipped_watch_frame_is_a_drop() {
        // The client saw 41 and the channel handed it 43: frame 42's dirty rows
        // exist nowhere else, so the row map is now missing them.
        assert!(watch_dropped_frames(41, 43));
        assert!(watch_dropped_frames(0, 7));
    }

    /// A `watch` receiver can be woken without a new value (a spurious
    /// `changed()`); re-reading the same sequence is not a drop and must not cost
    /// a full frame.
    #[test]
    fn the_same_watch_frame_twice_is_not_a_drop() {
        assert!(!watch_dropped_frames(42, 42));
    }

    /// The sequence can only go backwards if the reader is looking at a channel
    /// that was replaced under it. Whatever it holds is then unrelated to what is
    /// arriving, which is exactly the case a full frame repairs.
    #[test]
    fn a_sequence_that_went_backwards_is_a_drop() {
        assert!(watch_dropped_frames(42, 3));
        assert!(watch_dropped_frames(42, 0));
    }

    #[tokio::test]
    async fn publishing_assigns_consecutive_sequence_numbers() {
        let tx = new_grid_watch();
        let mut rx = tx.subscribe();
        assert_eq!(rx.borrow_and_update().seq, 0);

        publish_grid_frame(&tx, vec![1, 2, 3]);
        rx.changed().await.expect("sender is alive");
        {
            let slot = rx.borrow_and_update();
            assert_eq!(slot.seq, 1);
            assert_eq!(slot.frame, vec![1, 2, 3]);
        }

        publish_grid_frame(&tx, vec![4]);
        rx.changed().await.expect("sender is alive");
        assert_eq!(rx.borrow_and_update().seq, 2);
    }

    /// The exact browser-side defect: two frames published between reads, only the
    /// newest survives, and the reader can prove the older one is gone.
    #[tokio::test]
    async fn a_slow_reader_can_detect_the_frame_it_never_saw() {
        let tx = new_grid_watch();
        let mut rx = tx.subscribe();
        let mut last_seq = rx.borrow_and_update().seq;

        publish_grid_frame(&tx, vec![1]); // seq 1 — never read
        publish_grid_frame(&tx, vec![2]); // seq 2
        rx.changed().await.expect("sender is alive");
        let seq = rx.borrow_and_update().seq;

        assert_eq!(seq, 2);
        assert!(watch_dropped_frames(last_seq, seq));
        last_seq = seq;
        assert!(!watch_dropped_frames(last_seq, seq));
    }

    // --- Frame ordering (670-b9a2) ---
    //
    // Five producers serialize under the vt lock and publish after releasing it,
    // so the order frames reach a transport is not the order they were cut. The
    // resize path is the one that hurts: it serializes a FULL frame, and a full
    // frame that lands after a delta repaints the client with the older screen —
    // the "blank after zoom" the resize flush exists to prevent. Nothing about
    // the arrival order is observable at the transport, which is why the order is
    // stamped at the only place it exists: inside the critical section that cut
    // the bytes.

    #[test]
    fn a_frame_cut_earlier_cannot_overwrite_one_cut_later() {
        let tx = new_grid_watch();
        // Two producers, both still holding their own vt critical section.
        let resize = GridFrame::cut(vec![0xAA; 4]);
        let delta = GridFrame::cut(vec![0xBB; 2]);
        assert!(resize.order < delta.order, "cut order must be recorded");

        // Both released the lock; the one cut LAST wins the race to the channel,
        // then the older one arrives.
        assert_eq!(
            claim_grid_frame(&tx, delta.order, Some(delta.bytes.clone())),
            FrameOrder::Newest
        );
        assert_eq!(
            claim_grid_frame(&tx, resize.order, Some(resize.bytes)),
            FrameOrder::Stale
        );

        assert_eq!(
            tx.borrow().frame,
            delta.bytes,
            "a frame cut before the one already published repainted the newer screen"
        );
    }

    #[test]
    fn a_rejected_frame_does_not_move_the_sequence() {
        let tx = new_grid_watch();
        let older = GridFrame::cut(vec![1]);
        let newer = GridFrame::cut(vec![2]);
        claim_grid_frame(&tx, newer.order, Some(newer.bytes));
        let seq_after_newer = tx.borrow().seq;

        claim_grid_frame(&tx, older.order, Some(older.bytes));

        // The sequence counts what this channel published. Moving it for a frame
        // that was never published would show every reader a gap that did not
        // happen, and cost each of them a full-frame resync to close it.
        assert_eq!(tx.borrow().seq, seq_after_newer);
    }

    #[test]
    fn claiming_without_publishing_still_orders_the_next_frame() {
        let tx = new_grid_watch();
        let older = GridFrame::cut(vec![1]);
        let newer = GridFrame::cut(vec![2]);

        // Nobody was watching when the newer frame went out, so no bytes went
        // into the slot — but the desktop channel and this watch share one grid
        // and one damage set, so they share one ordering. A browser that connects
        // in between must not be handed the older frame as if it were current.
        assert_eq!(claim_grid_frame(&tx, newer.order, None), FrameOrder::Newest);
        assert_eq!(tx.borrow().seq, 0, "nothing was published");

        assert_eq!(
            claim_grid_frame(&tx, older.order, Some(older.bytes)),
            FrameOrder::Stale,
            "an order claimed by the other transport must still reject an older frame"
        );
        assert!(tx.borrow().frame.is_empty());
    }

    #[test]
    fn a_frame_cut_later_is_admitted_after_one_cut_earlier() {
        let tx = new_grid_watch();
        let first = GridFrame::cut(vec![1]);
        let second = GridFrame::cut(vec![2]);

        // The ordinary case, in the ordinary order: nothing is rejected.
        assert_eq!(
            claim_grid_frame(&tx, first.order, Some(first.bytes)),
            FrameOrder::Newest
        );
        assert_eq!(
            claim_grid_frame(&tx, second.order, Some(second.bytes.clone())),
            FrameOrder::Newest
        );
        assert_eq!(tx.borrow().frame, second.bytes);
        assert_eq!(tx.borrow().seq, 2);
    }

    // --- Frames withheld from a stalled WebView (670-b9a2) ---

    #[test]
    fn a_fresh_gate_owes_nothing() {
        let gate = GridGate::new();
        assert!(!gate.take_missed());
    }

    #[test]
    fn a_missed_frame_is_owed_exactly_once() {
        let gate = GridGate::new();
        // Several frames went to the WebSocket subscribers while the WebView was
        // behind. The debt is one full frame, not one per frame skipped.
        gate.note_missed();
        gate.note_missed();

        assert!(gate.take_missed(), "the debt must be reported");
        assert!(
            !gate.take_missed(),
            "paying it twice sends a full frame nobody needs"
        );
    }

    /// A hidden terminal deliberately never acks (see CanvasTerminal.onFrame), so
    /// the gate stays closed and the ticker's force-reset is the only thing that
    /// moves it. That must keep working frame after frame, not wedge.
    #[test]
    fn a_never_acking_frontend_still_gets_one_frame_per_force_reset() {
        let gate = GridGate::new();
        for _ in 0..10 {
            assert!(gate.is_open());
            gate.mark_sent();
            assert!(!gate.is_open());
            gate.abandon();
        }
        assert_eq!(gate.outstanding(), 0);
    }
}
