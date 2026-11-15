//! Outside macOS, no clipping container: the web views go straight into the
//! window, placed relative to its top-left corner.

use eframe::egui;
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};

pub struct Clip {
    window: RawWindowHandle,
}

impl Clip {
    pub fn new(frame: &eframe::Frame) -> Option<Clip> {
        Some(Clip { window: frame.window_handle().ok()?.as_raw() })
    }

    /// Nothing to move: the children never need placing again.
    pub fn place(&mut self, _rect: egui::Rect, _zoom: f32) -> bool {
        false
    }

    pub fn hide(&mut self) {}

    pub fn origin(&self) -> egui::Pos2 {
        egui::Pos2::ZERO
    }
}

impl HasWindowHandle for Clip {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // Safety: the app's window outlives the page holding this.
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}
