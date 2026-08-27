//! Persisted geometry for the main window, saved on our own schedule instead of
//! relying on `tauri-plugin-window-state`'s `SIZE` flag.
//!
//! That flag is deliberately excluded at the plugin's registration in `lib.rs`:
//! the plugin saves `inner_size()` but restores via `set_size()`, which (per
//! `tauri_runtime`) sets the window's INNER size while `outer_size()` reads the
//! OUTER (frame-included) size — a mismatch that compounds every restart under
//! `titleBarStyle: Overlay`'s full-size content view. `main` is denylisted from
//! the plugin entirely; every other window (`floating-*`, `panel-*`) still goes
//! through it unchanged.
//!
//! We own the round-trip on the quantity we actually care about (`outer_size()`)
//! and make it self-correcting: [`corrected_size`] in `lib.rs` measures the drift
//! from one `set_size` call and applies a one-step correction, so restored
//! geometry converges instead of drifting.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

const WINDOW_GEOMETRY_FILE: &str = "window-geometry.json";

/// A window's persisted geometry, keyed by window label in `window-geometry.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct WindowGeometry {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) maximized: bool,
    pub(crate) fullscreen: bool,
}

type WindowGeometryMap = HashMap<String, WindowGeometry>;

/// Load one window's saved geometry, if any.
pub(crate) fn load(label: &str) -> Option<WindowGeometry> {
    let map: WindowGeometryMap = crate::config::load_json_config(WINDOW_GEOMETRY_FILE);
    map.get(label).copied()
}

/// Persist one window's geometry, preserving any other window's entry.
/// A no-op when the geometry is unchanged from what's on disk, so the
/// periodic flush doesn't rewrite the file (and bump its mtime) every tick.
pub(crate) fn save(label: &str, geometry: WindowGeometry) -> Result<(), String> {
    let label = label.to_string();
    crate::config::ConfigFile::<WindowGeometryMap>::new(WINDOW_GEOMETRY_FILE).update(move |map| {
        if map.get(&label) == Some(&geometry) {
            return false;
        }
        map.insert(label.clone(), geometry);
        true
    })
}

struct TrackerState {
    geometry: WindowGeometry,
    dirty: bool,
}

/// Tracks the main window's live geometry between saves.
///
/// `record_size`/`record_position` are no-ops while maximized or fullscreen —
/// otherwise the OS-driven maximized size would overwrite the pre-maximize
/// geometry we actually want to restore to after the user un-maximizes.
/// `set_maximized`/`set_fullscreen` still mark the tracker dirty on their own
/// so the flag itself gets persisted.
pub(crate) struct WindowGeometryTracker {
    inner: Mutex<TrackerState>,
}

impl WindowGeometryTracker {
    pub(crate) fn new(initial: WindowGeometry) -> Self {
        Self {
            inner: Mutex::new(TrackerState {
                geometry: initial,
                dirty: false,
            }),
        }
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut TrackerState) -> R) -> R {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    pub(crate) fn record_size(&self, width: u32, height: u32) {
        self.with_state(|s| {
            if s.geometry.maximized || s.geometry.fullscreen {
                return;
            }
            if s.geometry.width != width || s.geometry.height != height {
                s.geometry.width = width;
                s.geometry.height = height;
                s.dirty = true;
            }
        });
    }

    pub(crate) fn record_position(&self, x: i32, y: i32) {
        self.with_state(|s| {
            if s.geometry.maximized || s.geometry.fullscreen {
                return;
            }
            if s.geometry.x != x || s.geometry.y != y {
                s.geometry.x = x;
                s.geometry.y = y;
                s.dirty = true;
            }
        });
    }

    pub(crate) fn set_maximized(&self, maximized: bool) {
        self.with_state(|s| {
            if s.geometry.maximized != maximized {
                s.geometry.maximized = maximized;
                s.dirty = true;
            }
        });
    }

    pub(crate) fn set_fullscreen(&self, fullscreen: bool) {
        self.with_state(|s| {
            if s.geometry.fullscreen != fullscreen {
                s.geometry.fullscreen = fullscreen;
                s.dirty = true;
            }
        });
    }

    /// Return the current geometry and clear the dirty flag, or `None` if
    /// nothing changed since the last call — the periodic flush task's cue to
    /// skip the write entirely.
    pub(crate) fn take_if_dirty(&self) -> Option<WindowGeometry> {
        self.with_state(|s| {
            if s.dirty {
                s.dirty = false;
                Some(s.geometry)
            } else {
                None
            }
        })
    }

    /// Current geometry regardless of dirty state — used for the final flush at
    /// `RunEvent::Exit`, which must persist even if the interval hasn't ticked
    /// since the last change.
    pub(crate) fn current(&self) -> WindowGeometry {
        self.with_state(|s| s.geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(x: i32, y: i32, width: u32, height: u32) -> WindowGeometry {
        WindowGeometry {
            x,
            y,
            width,
            height,
            maximized: false,
            fullscreen: false,
        }
    }

    #[test]
    fn record_size_marks_dirty_on_change() {
        let tracker = WindowGeometryTracker::new(geom(0, 0, 1200, 800));
        assert!(
            tracker.take_if_dirty().is_none(),
            "freshly created tracker is not dirty"
        );

        tracker.record_size(1400, 900);

        let g = tracker
            .take_if_dirty()
            .expect("size change must mark dirty");
        assert_eq!((g.width, g.height), (1400, 900));
        assert!(
            tracker.take_if_dirty().is_none(),
            "dirty flag clears after take"
        );
    }

    #[test]
    fn record_size_is_noop_when_unchanged() {
        let tracker = WindowGeometryTracker::new(geom(0, 0, 1200, 800));
        tracker.record_size(1200, 800);
        assert!(tracker.take_if_dirty().is_none());
    }

    #[test]
    fn record_position_marks_dirty_on_change() {
        let tracker = WindowGeometryTracker::new(geom(0, 0, 1200, 800));
        tracker.record_position(50, 60);
        let g = tracker
            .take_if_dirty()
            .expect("position change must mark dirty");
        assert_eq!((g.x, g.y), (50, 60));
    }

    #[test]
    fn size_and_position_are_ignored_while_maximized() {
        let tracker = WindowGeometryTracker::new(geom(100, 100, 1200, 800));
        tracker.set_maximized(true);
        tracker.take_if_dirty(); // consume the maximized-flip dirty flag

        tracker.record_size(1920, 1080);
        tracker.record_position(0, 0);

        assert!(
            tracker.take_if_dirty().is_none(),
            "size/position must not overwrite the pre-maximize geometry"
        );
        let current = tracker.current();
        assert_eq!((current.width, current.height), (1200, 800));
        assert_eq!((current.x, current.y), (100, 100));
    }

    #[test]
    fn size_and_position_resume_after_unmaximize() {
        let tracker = WindowGeometryTracker::new(geom(100, 100, 1200, 800));
        tracker.set_maximized(true);
        tracker.record_size(1920, 1080); // ignored while maximized
        tracker.set_maximized(false);
        tracker.take_if_dirty(); // consume the two maximized-flip dirty flags

        tracker.record_size(1300, 850);

        let g = tracker
            .take_if_dirty()
            .expect("size recorded again once un-maximized");
        assert_eq!((g.width, g.height), (1300, 850));
    }

    #[test]
    fn fullscreen_also_suppresses_size_and_position_recording() {
        let tracker = WindowGeometryTracker::new(geom(0, 0, 1200, 800));
        tracker.set_fullscreen(true);
        tracker.take_if_dirty();

        tracker.record_size(1920, 1080);

        assert!(tracker.take_if_dirty().is_none());
    }

    #[test]
    fn current_reflects_latest_geometry_even_when_not_dirty() {
        let tracker = WindowGeometryTracker::new(geom(0, 0, 1200, 800));
        tracker.record_size(1400, 900);
        tracker.take_if_dirty();
        assert_eq!(tracker.current().width, 1400);
    }

    #[test]
    fn window_geometry_map_round_trips_through_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let geometry = WindowGeometry {
            x: 10,
            y: 20,
            width: 1400,
            height: 900,
            maximized: false,
            fullscreen: false,
        };
        save("main", geometry).unwrap();

        assert_eq!(load("main"), Some(geometry));
        assert_eq!(load("other"), None);
    }

    #[test]
    fn save_is_a_noop_write_when_geometry_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let geometry = geom(10, 20, 1400, 900);
        save("main", geometry).unwrap();
        let path = dir.path().join(WINDOW_GEOMETRY_FILE);
        let mtime1 = std::fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        save("main", geometry).unwrap(); // identical — must not rewrite

        let mtime2 = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(mtime1, mtime2, "identical geometry must not touch the file");
    }

    #[test]
    fn save_preserves_other_windows_entries() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        save("main", geom(0, 0, 1200, 800)).unwrap();
        save("secondary", geom(50, 50, 900, 600)).unwrap();

        assert!(load("main").is_some());
        assert!(load("secondary").is_some());
    }

    #[test]
    fn load_returns_none_when_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        assert_eq!(load("main"), None);
    }
}
