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
        }
    }
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
    fn store_image(
        &self,
        _bytes: Arc<[u8]>,
        _mime: String,
        _intrinsic_width: u32,
        _intrinsic_height: u32,
    ) -> Option<Arc<crate::term::cell::ImageData>> {
        None
    }
}

/// Null sink for events.
pub struct VoidListener;

impl EventListener for VoidListener {}
