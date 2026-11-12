//! A native container that clips web views to a scrolling area.
//!
//! Web views are native views over the window: egui cannot clip them. Placed in
//! this container, sized to the visible part of a scroll area, they are cut at its
//! edges and can scroll out of sight like the rest of the page. The mouse wheel
//! over them scrolls the page, not their content.

use std::ptr::NonNull;

use block2::RcBlock;
use eframe::egui;
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventMask, NSView};
use objc2_foundation::{NSPoint, NSPointInRect, NSRect, NSSize};
use raw_window_handle::{AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};

pub struct Clip {
    view: Retained<NSView>,
    at: Option<(egui::Rect, f32)>,
    shown: bool,
}

impl Clip {
    pub fn new(frame: &eframe::Frame) -> Option<Clip> {
        let mtm = MainThreadMarker::new()?;
        let RawWindowHandle::AppKit(h) = frame.window_handle().ok()?.as_raw() else { return None };
        // Safety: the window's content view lives as long as the app.
        let parent: &NSView = unsafe { h.ns_view.cast().as_ref() };
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
        view.setClipsToBounds(true);
        parent.addSubview(&view);

        // Wheel events over the container go to the window's view, which scrolls the page.
        let (clip, page) = (view.clone(), parent.retain());
        let redirect = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let e = unsafe { event.as_ref() };
            let same_window = e.window(mtm).is_some_and(|w| clip.window().is_some_and(|cw| w == cw));
            let inside = same_window && !clip.isHidden() && NSPointInRect(clip.convertPoint_fromView(e.locationInWindow(), None), clip.bounds());
            if !inside {
                return event.as_ptr();
            }
            page.scrollWheel(e);
            std::ptr::null_mut()
        });
        // The monitor lives as long as the app, like the container.
        std::mem::forget(unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::ScrollWheel, &redirect) });
        Some(Clip { view, at: None, shown: true })
    }

    /// Places the container over `rect`; true when it moved (its children must be placed again).
    pub fn place(&mut self, rect: egui::Rect, zoom: f32) -> bool {
        if !self.shown {
            self.view.setHidden(false);
            self.shown = true;
        }
        if self.at == Some((rect, zoom)) {
            return false;
        }
        self.at = Some((rect, zoom));
        // The window's content view is flipped: y goes down, like egui.
        let r = NSRect::new(
            NSPoint::new((rect.left() * zoom) as f64, (rect.top() * zoom) as f64),
            NSSize::new((rect.width() * zoom) as f64, (rect.height() * zoom) as f64),
        );
        self.view.setFrame(r);
        true
    }

    pub fn hide(&mut self) {
        if self.shown {
            self.view.setHidden(true);
            self.shown = false;
        }
    }

    /// Top-left corner, in egui points: children are placed relative to it.
    pub fn origin(&self) -> egui::Pos2 {
        self.at.map_or(egui::Pos2::ZERO, |(r, _)| r.min)
    }
}

impl HasWindowHandle for Clip {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let ptr = NonNull::from(&*self.view).cast();
        // Safety: the view outlives the borrow.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(ptr))) })
    }
}
