//! Per-session inline-image store (iTerm2 OSC 1337 / Kitty graphics protocol —
//! color-tools plan). Protocol parsing lives in Phases 2/3 (`vte`/`alacritty_terminal`
//! OSC/APC dispatch); this module is the storage seam those handlers call into,
//! plus what the `terminal_image_bytes` transport surface reads from.
//!
//! # The store holds strong references; deletion is explicit
//!
//! [`ImageData`](alacritty_terminal::term::cell::ImageData) is held by an
//! `Arc`, cloned directly into every
//! [`ImageCellRef`](alacritty_terminal::term::cell::ImageCellRef) that shows
//! one of its tiles (see that type's own doc comment) — cells never need to
//! ask this store for bytes they already display. But the store's *own* map
//! holds a **strong** `Arc` too, not a `Weak` one: Kitty's `a=t` (transmit
//! without display) is a first-class, common case — a client transmits an
//! image now and may `a=p` (place) it later, possibly more than once, or
//! never at all until it explicitly `a=d`-deletes it. If the store held only
//! a `Weak` ref, an image transmitted-but-not-yet-displayed would have zero
//! strong references anywhere and be freed before any later `a=p` could find
//! it — a real bug this design avoids. Freeing therefore requires an
//! explicit `forget`/`forget_all` (Kitty `a=d`) removing the store's own
//! reference; a cell that separately holds the same `Arc` (because it was
//! displayed at some point) keeps the bytes alive independently until *it*
//! is overwritten, same as before. A byte cap still exists, enforced against
//! the store's own held total, as a transmission-time *refusal* rather than
//! a silent eviction of anything.

use alacritty_terminal::term::cell::ImageData;
use std::collections::HashMap;
use std::sync::Arc;

/// Per-session cap on the combined size of images this store currently
/// holds a strong reference to. Deliberately generous — a handful of real
/// screenshots/photos, not a hard architectural limit.
pub(crate) const MAX_SESSION_IMAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ImageStoreError {
    /// Accepting this transmission would exceed `MAX_SESSION_IMAGE_BYTES` of
    /// still-live image data for this session.
    CapExceeded {
        incoming: usize,
        live: usize,
        cap: usize,
    },
}

impl std::fmt::Display for ImageStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageStoreError::CapExceeded {
                incoming,
                live,
                cap,
            } => write!(
                f,
                "image transmission of {incoming} bytes would exceed the \
                 {cap}-byte session cap ({live} bytes currently live)"
            ),
        }
    }
}

/// Per-session inline-image store. Not a cache — see module docs.
#[derive(Default)]
pub(crate) struct ImageStore {
    next_image_id: u32,
    by_id: HashMap<u32, Arc<ImageData>>,
}

impl ImageStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Combined size of every image this store currently holds a strong
    /// reference to (see module docs for why that's "currently held", not
    /// "currently displayed somewhere").
    pub(crate) fn live_bytes(&self) -> usize {
        self.by_id.values().map(|d| d.bytes.len()).sum()
    }

    /// Register a new image, or refuse if doing so would exceed the
    /// per-session live-byte cap.
    ///
    /// iTerm2 has no client-chosen image identity, so its callers pass
    /// `client_id: None` and get an auto-allocated id back. Kitty's `i=` is
    /// client-chosen and later referenced by `a=p`/`a=d`, so its callers
    /// pass `Some(id)`; re-transmitting the same id replaces the previous
    /// entry (Kitty allows this) — a cell that already displayed the old
    /// `Arc` keeps its own clone and is unaffected, it just becomes
    /// unreachable via `get`/`bytes` under that id going forward.
    pub(crate) fn store(
        &mut self,
        client_id: Option<u32>,
        bytes: Arc<[u8]>,
        mime: String,
        intrinsic_width: u32,
        intrinsic_height: u32,
    ) -> Result<Arc<ImageData>, ImageStoreError> {
        let incoming = bytes.len();
        let live = self.live_bytes();
        if live.saturating_add(incoming) > MAX_SESSION_IMAGE_BYTES {
            return Err(ImageStoreError::CapExceeded {
                incoming,
                live,
                cap: MAX_SESSION_IMAGE_BYTES,
            });
        }
        let image_id = match client_id {
            Some(id) => id,
            None => {
                self.next_image_id = self.next_image_id.wrapping_add(1);
                self.next_image_id
            }
        };
        let data = Arc::new(ImageData {
            image_id,
            bytes,
            mime,
            intrinsic_width,
            intrinsic_height,
        });
        self.by_id.insert(image_id, Arc::clone(&data));
        Ok(data)
    }

    /// Look up an image's bytes by id, for the `terminal_image_bytes` fetch
    /// surface. `None` if the id is unknown or has been forgotten (`a=d`) —
    /// the caller should treat both cases identically (a 404-shaped
    /// response), not try to distinguish "never existed" from "forgotten".
    pub(crate) fn bytes(&self, image_id: u32) -> Option<Arc<[u8]>> {
        self.by_id.get(&image_id).map(|d| Arc::clone(&d.bytes))
    }

    /// Look up a previously stored image by id, for Kitty's `a=p` (place an
    /// already-transmitted image). `None` if unknown or forgotten.
    pub(crate) fn get(&self, image_id: u32) -> Option<Arc<ImageData>> {
        self.by_id.get(&image_id).cloned()
    }

    /// Forget an image id (Kitty `a=d`), so a later `get`/`bytes` for it
    /// returns `None` and it no longer counts against the byte cap. Does
    /// **not** touch any cell already showing this image — those hold their
    /// own clone of the `Arc` and keep displaying it until naturally
    /// overwritten, independent of this store (both "retain" and "free" `d=`
    /// variants are treated identically here, since a cell's own bytes are
    /// freed by ordinary Rust ownership once nothing references them any
    /// more, this store included).
    pub(crate) fn forget(&mut self, image_id: u32) {
        self.by_id.remove(&image_id);
    }

    /// Forget every image this store currently knows about (Kitty `a=d,d=a`
    /// or `d=A`). Same caveat as `forget`.
    pub(crate) fn forget_all(&mut self) {
        self.by_id.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_then_fetch_round_trips_bytes() {
        let mut store = ImageStore::new();
        let data = store
            .store(
                None,
                Arc::from(vec![1u8, 2, 3, 4]),
                "image/png".to_string(),
                10,
                10,
            )
            .expect("under cap");
        assert_eq!(
            store.bytes(data.image_id).as_deref(),
            Some(&[1u8, 2, 3, 4][..])
        );
    }

    #[test]
    fn unknown_id_returns_none() {
        let store = ImageStore::new();
        assert_eq!(store.bytes(9999), None);
    }

    #[test]
    fn each_auto_allocated_store_call_gets_a_distinct_id() {
        let mut store = ImageStore::new();
        let a = store
            .store(None, Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        let b = store
            .store(None, Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        assert_ne!(a.image_id, b.image_id);
    }

    /// Kitty's `i=` is client-chosen, not server-allocated — `store` must
    /// respect it exactly, and a later `get` must find it under that same id.
    #[test]
    fn client_chosen_id_is_respected_and_lookupable() {
        let mut store = ImageStore::new();
        let data = store
            .store(
                Some(42),
                Arc::from(vec![1u8, 2, 3]),
                "image/png".to_string(),
                1,
                1,
            )
            .unwrap();
        assert_eq!(data.image_id, 42);
        assert_eq!(store.get(42).map(|d| d.image_id), Some(42));
    }

    /// Re-transmitting the same client id (Kitty allows this) replaces the
    /// lookup entry without touching any cell that still holds the old
    /// `Arc` directly — that cell's own strong reference is what's supposed
    /// to keep the old bytes alive until it's overwritten, not this store.
    #[test]
    fn retransmitting_the_same_client_id_replaces_the_lookup_entry() {
        let mut store = ImageStore::new();
        let old = store
            .store(Some(7), Arc::from(vec![1u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        let new = store
            .store(Some(7), Arc::from(vec![2u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        assert_eq!(store.get(7).map(|d| d.bytes.to_vec()), Some(vec![2u8]));
        // The old Arc is still perfectly valid on its own -- a cell holding
        // it directly would keep showing the old bytes.
        assert_eq!(old.bytes.to_vec(), vec![1u8]);
        assert_eq!(new.bytes.to_vec(), vec![2u8]);
    }

    #[test]
    fn forget_removes_one_image_from_lookup() {
        let mut store = ImageStore::new();
        let a = store
            .store(Some(1), Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        let _b = store
            .store(Some(2), Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        store.forget(1);
        assert_eq!(store.get(1), None);
        assert!(store.get(2).is_some());
        drop(a);
    }

    #[test]
    fn forget_all_clears_every_lookup_entry() {
        let mut store = ImageStore::new();
        store
            .store(Some(1), Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        store
            .store(Some(2), Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        store.forget_all();
        assert_eq!(store.get(1), None);
        assert_eq!(store.get(2), None);
    }

    /// The store itself holds a strong reference — dropping a caller's own
    /// clone of the `Arc` (e.g. a transmit-only `a=t` handler that never
    /// attaches it to a cell) must NOT free it. This is the exact bug the
    /// original weak-ref design had: Kitty's `a=t` transmits without
    /// displaying, so nothing else would hold a reference, and the image
    /// would vanish before any later `a=p` (place) could find it.
    #[test]
    fn dropping_a_callers_own_clone_does_not_free_the_stored_image() {
        let mut store = ImageStore::new();
        let big = vec![0u8; 1024];
        let data = store
            .store(None, Arc::from(big.clone()), "image/png".to_string(), 1, 1)
            .unwrap();
        let image_id = data.image_id;
        assert_eq!(store.live_bytes(), 1024);

        drop(data); // e.g. a transmit-only handler that never displays it

        assert_eq!(
            store.live_bytes(),
            1024,
            "the store's own reference must keep it live"
        );
        assert_eq!(
            store.bytes(image_id).as_deref().map(<[u8]>::len),
            Some(1024),
            "still fetchable — a later a=p must be able to find it"
        );
    }

    #[test]
    fn transmission_over_the_cap_is_refused_without_mutating_state() {
        let mut store = ImageStore::new();
        let oversized = vec![0u8; MAX_SESSION_IMAGE_BYTES + 1];
        let err = store
            .store(None, Arc::from(oversized), "image/png".to_string(), 1, 1)
            .unwrap_err();
        assert_eq!(
            err,
            ImageStoreError::CapExceeded {
                incoming: MAX_SESSION_IMAGE_BYTES + 1,
                live: 0,
                cap: MAX_SESSION_IMAGE_BYTES,
            }
        );
        assert_eq!(
            store.live_bytes(),
            0,
            "a refused transmission must not be registered"
        );
    }

    #[test]
    fn a_still_referenced_image_is_never_evicted_by_a_later_transmission() {
        let mut store = ImageStore::new();
        // Fill most of the cap with one still-referenced image.
        let kept = store
            .store(
                None,
                Arc::from(vec![0u8; MAX_SESSION_IMAGE_BYTES - 100]),
                "image/png".to_string(),
                1,
                1,
            )
            .unwrap();
        // A second transmission that would push past the cap is refused —
        // NOT satisfied by silently evicting `kept`, which is still live.
        let err = store
            .store(
                None,
                Arc::from(vec![0u8; 200]),
                "image/png".to_string(),
                1,
                1,
            )
            .unwrap_err();
        assert!(matches!(err, ImageStoreError::CapExceeded { .. }));
        assert_eq!(
            store.bytes(kept.image_id).as_deref().map(<[u8]>::len),
            Some(MAX_SESSION_IMAGE_BYTES - 100),
            "the still-referenced image must survive a refused sibling transmission"
        );
    }

    /// The real eviction path now: `forget` drops the store's own reference,
    /// and once no `Arc` clone survives anywhere else (here, the caller's
    /// `data` handle is the only other one, and it's dropped too), the bytes
    /// are actually freed — `Arc::strong_count` proves it rather than just
    /// checking `get`/`bytes` return `None`.
    #[test]
    fn forget_plus_no_other_reference_actually_frees_the_bytes() {
        let mut store = ImageStore::new();
        let data = store
            .store(
                Some(1),
                Arc::from(vec![0u8; 64]),
                "image/png".to_string(),
                1,
                1,
            )
            .unwrap();
        assert_eq!(Arc::strong_count(&data), 2, "the store and this handle");

        store.forget(1);
        assert_eq!(
            Arc::strong_count(&data),
            1,
            "forget must drop the store's own reference"
        );
        assert_eq!(store.live_bytes(), 0);

        drop(data); // the only other reference
        // Nothing left to assert on the Arc itself (it's gone), but the
        // store-level view must agree it's unreachable.
        assert_eq!(store.get(1), None);
    }
}
