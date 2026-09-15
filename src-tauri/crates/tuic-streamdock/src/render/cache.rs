//! Content-keyed render cache. `KeyFace -> Arc<[u8]>` (encoded JPEG) behind
//! an LRU, so a face seen before — e.g. a session flipping between
//! `Working` and `CompletedUnread` repeatedly — is encoded once, not once
//! per occurrence. This is the *second* level of dedup; the coordinator's
//! own `last_pushed[slot]` check (outside this crate boundary, in
//! `coordinator.rs`) is the first, and skips even a cache lookup for a slot
//! whose face hasn't changed since the last tick.

use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;

use crate::render::face::KeyFace;
use crate::render::text::FontFace;

pub struct RenderCache {
    font: FontFace,
    cache: LruCache<KeyFace, Arc<[u8]>>,
    key_px: u32,
    jpeg_quality: u8,
}

impl RenderCache {
    pub fn new(key_px: u32, jpeg_quality: u8, capacity: usize) -> Self {
        Self {
            font: FontFace::bundled(),
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN)),
            key_px,
            jpeg_quality,
        }
    }

    /// Renders `face`, or returns the cached encoding if this exact face
    /// has been rendered before.
    pub fn render(&mut self, face: &KeyFace) -> Arc<[u8]> {
        if let Some(hit) = self.cache.get(face) {
            return hit.clone();
        }
        let jpeg: Arc<[u8]> =
            crate::render::draw::render_face_jpeg(&self.font, face, self.key_px, self.jpeg_quality)
                .into();
        self.cache.put(face.clone(), jpeg.clone());
        jpeg
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::face::{Glyph, Label};
    use crate::render::palette::FaceState;

    fn face(primary: &str) -> KeyFace {
        KeyFace {
            state: FaceState::Idle,
            glyph: Glyph::None,
            primary: Label::from_str_truncated(primary),
            secondary: Label::new(),
            badge: None,
        }
    }

    #[test]
    fn repeated_face_is_a_cache_hit_not_a_reencode() {
        let mut cache = RenderCache::new(64, 90, 16);
        let a = cache.render(&face("tc-1"));
        assert_eq!(cache.len(), 1);
        let b = cache.render(&face("tc-1"));
        assert_eq!(
            cache.len(),
            1,
            "rendering the same face twice must not grow the cache"
        );
        assert!(
            Arc::ptr_eq(&a, &b),
            "the second call must return the exact cached Arc, not a fresh encode"
        );
    }

    #[test]
    fn distinct_faces_both_cache() {
        let mut cache = RenderCache::new(64, 90, 16);
        cache.render(&face("tc-1"));
        cache.render(&face("tc-2"));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn eviction_respects_capacity() {
        let mut cache = RenderCache::new(64, 90, 2);
        cache.render(&face("tc-1"));
        cache.render(&face("tc-2"));
        cache.render(&face("tc-3"));
        assert_eq!(
            cache.len(),
            2,
            "a capacity-2 cache must never hold more than 2 entries"
        );
    }
}
