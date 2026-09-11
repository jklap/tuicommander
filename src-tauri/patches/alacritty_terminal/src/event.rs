use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::process::ExitStatus;
use std::sync::Arc;

use crate::term::ClipboardType;
use crate::vte::ansi::Rgb;

/// Terminal event.
///
/// These events instruct the UI over changes that can't be handled by the terminal emulation layer
/// itself.
#[derive(Clone)]
pub enum Event {
    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// The attached function is a formatter which will correctly transform the clipboard content
    /// into the expected escape sequence format.
    ClipboardLoad(
        ClipboardType,
        Arc<dyn Fn(&str) -> String + Sync + Send + 'static>,
    ),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// The attached function is a formatter which will correctly transform the RGB color into the
    /// expected escape sequence format.
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),

    /// Write some text to the PTY.
    PtyWrite(String),

    /// Request to write the text area size.
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),

    /// Request to write the cell size in pixels (`CSI 16 t`).
    CellSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    /// New terminal content available.
    Wakeup,

    /// Terminal bell ring.
    Bell,

    /// Shutdown request.
    Exit,

    /// Child process exited.
    ChildExit(ExitStatus),

    /// OSC 133 shell integration marker.
    Osc133 {
        command: char,
        params: String,
        line: usize,
    },

    /// OSC 7 current working directory (file://hostname/path).
    Osc7(String),

    /// OSC 7770 TUIC protocol event (verb=payload).
    Tuic {
        verb: String,
        payload: String,
        line: usize,
    },

    /// iTerm2 OSC 1337 `StealFocus` — bring the terminal application to the
    /// foreground.
    RequestFocus,

    /// iTerm2 OSC 1337 `RequestAttention=<value>`. `value` is one of "yes",
    /// "once", "no", or "fireworks".
    RequestAttention(String),

    /// iTerm2 OSC 1337 `OpenURL=:<base64>` — the decoded URL.
    OpenUrl(String),

    /// An inline-image placement was just created or moved into place
    /// (color-tools plan, Phase 5 — frontend renderer). Emitted once by
    /// `reserve_image_footprint` right after it attaches `ImageCellRef`s to
    /// the grid, carrying everything a renderer needs to paint it without a
    /// separate lookup: which image, where, how big, and at what z-band.
    ImagePlacement(ImagePlacementInfo),

    /// Every previously-announced placement should be treated as gone (e.g.
    /// an alt-screen switch, which by design never scrolls or reserves
    /// image footprint on the alt screen, or `a=d,d=A/a`). A renderer
    /// re-hydrates its placement set via a fresh query rather than trying to
    /// diff against whatever it had before.
    ImagePlacementsCleared,
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({ty:?}, {text})"),
            Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({ty:?})"),
            Event::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
            Event::CellSizeRequest(_) => write!(f, "CellSizeRequest"),
            Event::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
            Event::PtyWrite(text) => write!(f, "PtyWrite({text})"),
            Event::Title(title) => write!(f, "Title({title})"),
            Event::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::ResetTitle => write!(f, "ResetTitle"),
            Event::Wakeup => write!(f, "Wakeup"),
            Event::Bell => write!(f, "Bell"),
            Event::Exit => write!(f, "Exit"),
            Event::ChildExit(status) => write!(f, "ChildExit({status:?})"),
            Event::Osc133 {
                command,
                params,
                line,
            } => write!(f, "Osc133({command}, {params:?}, line={line})"),
            Event::Osc7(url) => write!(f, "Osc7({url})"),
            Event::Tuic {
                verb,
                payload,
                line,
            } => write!(f, "Tuic({verb}={payload}, line={line})"),
            Event::RequestFocus => write!(f, "RequestFocus"),
            Event::RequestAttention(value) => write!(f, "RequestAttention({value})"),
            Event::OpenUrl(url) => write!(f, "OpenUrl({url})"),
            Event::ImagePlacement(info) => write!(f, "ImagePlacement({info:?})"),
            Event::ImagePlacementsCleared => write!(f, "ImagePlacementsCleared"),
        }
    }
}

/// Everything a renderer needs to paint one inline-image placement, carried
/// on [`Event::ImagePlacement`] (color-tools plan, Phase 5).
///
/// `abs_row` is the *eviction-stable* absolute row — `history_base +
/// grid_relative` at the moment of reservation, the same addressing
/// convention `terminal_grid.rs`'s `hyperlink_span` already uses for the
/// identical reason: it only ever grows, so a placement scrolled deep into
/// history is still identified consistently even after older lines are
/// evicted from the live `Grid`'s bounded history buffer. Converting it back
/// to an on-screen row is the transport layer's job (it already tracks
/// `history_base`/`history_size`/`display_offset` for the binary grid frame),
/// not this crate's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePlacementInfo {
    pub placement_id: u32,
    pub image_id: u32,
    pub abs_row: u32,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
    pub z_index: i32,
}

/// Byte sequences are sent to a `Notify` in response to some events.
pub trait Notify {
    /// Notify that an escape sequence should be written to the PTY.
    ///
    /// TODO this needs to be able to error somehow.
    fn notify<B: Into<Cow<'static, [u8]>>>(&self, _: B);
}

#[derive(Copy, Clone, Debug)]
pub struct WindowSize {
    pub num_lines: u16,
    pub num_cols: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

/// Types that are interested in when the display is resized.
pub trait OnResize {
    fn on_resize(&mut self, window_size: WindowSize);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn send_event(&self, _event: Event) {}

    /// Current cell pixel size (and grid dimensions), for computing an inline
    /// image's cell footprint from a pixel/percent size spec (color-tools
    /// plan). Default returns all zeros — a listener that doesn't track this
    /// (e.g. `VoidListener`, the `ref.rs` fixture-replay `Mock`) simply can't
    /// answer `px`/`%`-based image sizing correctly, which is fine for those
    /// callers since neither exercises inline images.
    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: 0,
            num_cols: 0,
            cell_width: 0,
            cell_height: 0,
        }
    }

    /// Register a newly transmitted inline image (color-tools plan) and
    /// return a handle to it, or `None` if refused (e.g. over a per-session
    /// byte cap). Default refuses everything — only a listener that actually
    /// owns image storage (the app's `TermEventCollector`) can accept one.
    ///
    /// `client_id`: iTerm2 has no client-chosen image identity, so its
    /// caller passes `None` and gets an auto-allocated id back. Kitty's `i=`
    /// is client-chosen and later referenced by `a=p`/`a=d`, so its caller
    /// passes `Some(id)`.
    fn store_image(
        &self,
        _client_id: Option<u32>,
        _bytes: Arc<[u8]>,
        _mime: String,
        _intrinsic_width: u32,
        _intrinsic_height: u32,
    ) -> Option<Arc<crate::term::cell::ImageData>> {
        None
    }

    /// Look up a previously stored image by id (Kitty `a=p`: place an
    /// already-transmitted image). `None` if unknown or evicted. Default
    /// matches `store_image`'s default of "no storage backing this
    /// listener."
    fn image_by_id(&self, _image_id: u32) -> Option<Arc<crate::term::cell::ImageData>> {
        None
    }

    /// Forget an image id (Kitty `a=d`) so `image_by_id`/`store_image`'s
    /// underlying lookup no longer finds it. Does not affect any cell
    /// already showing it — see `terminal_images::ImageStore::forget`.
    fn forget_image(&self, _image_id: u32) {}

    /// Forget every image (Kitty `a=d,d=a`/`d=A`).
    fn forget_all_images(&self) {}

    /// Read the payload bytes for Kitty's `t=f` (plain file) or `t=t`
    /// (temp file, delete after read) transmission mediums (color-tools
    /// plan, Phase 6). `path` is the already-base64-decoded byte string the
    /// wire format carries in the payload position for these mediums
    /// (a filesystem path, not pixel data). `delete_after` is `true` only
    /// for `t=t`.
    ///
    /// Deliberately not implemented in this crate: reading a client-chosen
    /// path (and, for `t=t`, conditionally deleting it) is exactly the kind
    /// of OS/filesystem-specific, security-sensitive operation the
    /// embedding application owns (same reasoning as `store_image` above) —
    /// see `terminal_images::read_file_medium` in the app crate for the
    /// actual implementation and its safety guards. Default refuses
    /// everything, matching every other storage-backed method here.
    fn read_file_medium(&self, _path: &[u8], _delete_after: bool) -> Option<Vec<u8>> {
        None
    }

    /// Read the payload bytes for Kitty's `t=s` (POSIX/Windows shared
    /// memory) transmission medium (color-tools plan, Phase 6). `name` is
    /// the already-base64-decoded shared-memory segment name. Same
    /// delegation reasoning as `read_file_medium`.
    fn read_shm_medium(&self, _name: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Register a not-yet-decoded inline image and return a placeholder
    /// handle to it immediately (color-tools plan: Kitty decode deferred off
    /// the `vt_log` lock) — or `None` if refused. Unlike `store_image`, this
    /// never fails on a byte cap (no bytes exist yet to check); a listener
    /// would only refuse if it has no image storage at all, matching
    /// `store_image`'s default. `mime`/`intrinsic_width`/`intrinsic_height`
    /// are already fully known at this point (derived from the wire format
    /// alone, or `s=`/`v=` for raw formats) — only the pixel bytes remain
    /// pending.
    fn store_pending_image(
        &self,
        _client_id: Option<u32>,
        _mime: String,
        _intrinsic_width: u32,
        _intrinsic_height: u32,
    ) -> Option<Arc<crate::term::cell::ImageData>> {
        None
    }

    /// Queue a `store_pending_image`-created placeholder's actual decode
    /// work (base64, medium read, zlib inflate, format trim) to run once the
    /// caller's lock is dropped. Default no-op, matching every other
    /// storage-backed method here — a listener that accepts this without
    /// ever draining it would just leave the placeholder pending forever,
    /// which is a safe (if useless) degradation, not a crash.
    fn queue_kitty_decode_job(&self, _job: crate::term::kitty::PendingKittyDecodeJob) {}
}

/// Null sink for events.
pub struct VoidListener;

impl EventListener for VoidListener {}
