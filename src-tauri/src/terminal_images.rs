//! Per-session inline-image store (iTerm2 OSC 1337 / Kitty graphics protocol —
//! color-tools plan). Protocol parsing lives in Phases 2/3 (`vte`/`alacritty_terminal`
//! OSC/APC dispatch); this module is the storage seam those handlers call into,
//! plus what the `terminal_image_bytes` transport surface reads from.
//!
//! # Eviction is ordinary Rust ownership, not a cache policy
//!
//! [`ImageData`](alacritty_terminal::term::cell::ImageData) is held by an `Arc`
//! directly inside every [`ImageCellRef`](alacritty_terminal::term::cell::ImageCellRef)
//! that shows one of its tiles (see that type's own doc comment). `ImageStore`
//! itself keeps only [`Weak`] references — it does not keep anything alive.
//! Once no cell (main screen or scrollback) references an image any more, its
//! last strong `Arc` drops and the bytes are freed automatically. A byte cap
//! still exists, but as a *refusal* at transmission time (`store` returns
//! `Err` rather than silently evicting something a live placement still
//! needs).

use alacritty_terminal::term::cell::ImageData;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

/// Per-session cap on the combined size of currently-live (still referenced)
/// images. Deliberately generous — a handful of real screenshots/photos, not
/// a hard architectural limit — since the real protection against runaway
/// memory is refcount-driven eviction, not this cap.
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
    by_id: HashMap<u32, Weak<ImageData>>,
}

impl ImageStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Bytes currently referenced by images with at least one live cell
    /// reference. Prunes dead entries as a side effect, so this also bounds
    /// `by_id`'s size to the number of currently-live images, not the number
    /// ever transmitted.
    pub(crate) fn live_bytes(&mut self) -> usize {
        let mut total = 0usize;
        self.by_id.retain(|_, weak| match weak.upgrade() {
            Some(arc) => {
                total += arc.bytes.len();
                true
            }
            None => false,
        });
        total
    }

    /// Allocate a new image id and register it, or refuse if doing so would
    /// exceed the per-session live-byte cap. On success the caller (an OSC
    /// 1337 / Kitty dispatch handler) attaches the returned `Arc` to one or
    /// more cells via `Cell::set_image_ref`; if it never does (e.g. the
    /// escape sequence turned out to be malformed), the `Arc` returned here
    /// is the only reference and is freed as soon as the caller drops it —
    /// no explicit rollback needed.
    pub(crate) fn store(
        &mut self,
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
        self.next_image_id = self.next_image_id.wrapping_add(1);
        let image_id = self.next_image_id;
        let data = Arc::new(ImageData {
            image_id,
            bytes,
            mime,
            intrinsic_width,
            intrinsic_height,
        });
        self.by_id.insert(image_id, Arc::downgrade(&data));
        Ok(data)
    }

    /// Look up an image's bytes by id, for the `terminal_image_bytes` fetch
    /// surface. `None` if the id is unknown or the image has already been
    /// evicted (no cell references it any more) — the caller should treat
    /// both cases identically (a 404-shaped response), not try to
    /// distinguish "never existed" from "evicted".
    pub(crate) fn bytes(&self, image_id: u32) -> Option<Arc<[u8]>> {
        self.by_id
            .get(&image_id)?
            .upgrade()
            .map(|d| Arc::clone(&d.bytes))
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
    fn each_store_call_gets_a_distinct_id() {
        let mut store = ImageStore::new();
        let a = store
            .store(Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        let b = store
            .store(Arc::from(vec![0u8]), "image/png".to_string(), 1, 1)
            .unwrap();
        assert_ne!(a.image_id, b.image_id);
    }

    /// The core eviction property this whole design exists for: once the
    /// last strong reference to an image's `Arc<ImageData>` drops — the same
    /// event a `CellExtra.image` being cleared or overwritten would trigger —
    /// its bytes become unreachable through the store, and the byte budget
    /// it occupied is freed for a later transmission, with no explicit
    /// eviction call.
    #[test]
    fn dropping_the_last_strong_ref_frees_the_slot() {
        let mut store = ImageStore::new();
        let big = vec![0u8; 1024];
        let data = store
            .store(Arc::from(big.clone()), "image/png".to_string(), 1, 1)
            .unwrap();
        let image_id = data.image_id;
        assert_eq!(store.live_bytes(), 1024);

        drop(data); // simulates the last CellExtra referencing it being cleared

        assert_eq!(
            store.live_bytes(),
            0,
            "freed image must not count against the cap"
        );
        assert_eq!(
            store.bytes(image_id),
            None,
            "an evicted image's bytes must not be fetchable"
        );
    }

    #[test]
    fn transmission_over_the_cap_is_refused_without_mutating_state() {
        let mut store = ImageStore::new();
        let oversized = vec![0u8; MAX_SESSION_IMAGE_BYTES + 1];
        let err = store
            .store(Arc::from(oversized), "image/png".to_string(), 1, 1)
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
                Arc::from(vec![0u8; MAX_SESSION_IMAGE_BYTES - 100]),
                "image/png".to_string(),
                1,
                1,
            )
            .unwrap();
        // A second transmission that would push past the cap is refused —
        // NOT satisfied by silently evicting `kept`, which is still live.
        let err = store
            .store(Arc::from(vec![0u8; 200]), "image/png".to_string(), 1, 1)
            .unwrap_err();
        assert!(matches!(err, ImageStoreError::CapExceeded { .. }));
        assert_eq!(
            store.bytes(kept.image_id).as_deref().map(<[u8]>::len),
            Some(MAX_SESSION_IMAGE_BYTES - 100),
            "the still-referenced image must survive a refused sibling transmission"
        );
    }
}
